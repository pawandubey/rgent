use std::{
    convert::Infallible,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{
    header::{HeaderValue, ALLOW, CONTENT_TYPE, LOCATION},
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

#[derive(Debug, PartialEq, Eq)]
enum Target {
    File(PathBuf),
    Redirect(String),
    NotFound,
}

pub fn serve(output_dir: PathBuf, port: u16) -> Result<()> {
    let runtime =
        tokio::runtime::Runtime::new().context("Failed to start async runtime for preview")?;

    runtime.block_on(async {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .with_context(|| format!("Failed to start preview server on port {}", port))?;

        println!(
            "Previewing {:?} at http://localhost:{} (press Ctrl+C to stop)",
            output_dir, port
        );

        serve_connections(listener, output_dir).await;
        Ok(())
    })
}

async fn serve_connections(listener: TcpListener, output_dir: PathBuf) {
    let output_dir = Arc::new(output_dir);

    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                eprintln!("Failed to accept preview connection: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        let output_dir = Arc::clone(&output_dir);
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let output_dir = Arc::clone(&output_dir);
                async move { Ok::<_, Infallible>(handle_request(&request, &output_dir).await) }
            });

            if let Err(e) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                eprintln!("Error serving preview connection: {}", e);
            }
        });
    }
}

async fn handle_request<B>(request: &Request<B>, output_dir: &Path) -> Response<Full<Bytes>> {
    let method = request.method();
    let path = request.uri().path();

    if method != Method::GET && method != Method::HEAD {
        println!("{} {} -> 405", method, path);
        let mut response = text_response(StatusCode::METHOD_NOT_ALLOWED, "405 Method Not Allowed");
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
        return response;
    }

    match resolve_target(output_dir, path) {
        Target::File(file_path) => match tokio::fs::read(&file_path).await {
            Ok(contents) => {
                println!("{} {} -> 200 {:?}", method, path, file_path);
                let mut response = Response::new(Full::new(Bytes::from(contents)));
                response.headers_mut().insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static(content_type(&file_path)),
                );
                response
            }
            Err(e) => {
                eprintln!(
                    "{} {} -> 500 failed to read {:?}: {}",
                    method, path, file_path, e
                );
                text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 Internal Server Error",
                )
            }
        },
        Target::Redirect(location) => match HeaderValue::from_str(&location) {
            Ok(location_header) => {
                println!("{} {} -> 301 {}", method, path, location);
                let mut response = Response::new(Full::default());
                *response.status_mut() = StatusCode::MOVED_PERMANENTLY;
                response.headers_mut().insert(LOCATION, location_header);
                response
            }
            Err(e) => {
                eprintln!(
                    "{} {} -> 500 invalid redirect location {:?}: {}",
                    method, path, location, e
                );
                text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 Internal Server Error",
                )
            }
        },
        Target::NotFound => {
            println!("{} {} -> 404", method, path);
            text_response(StatusCode::NOT_FOUND, "404 Not Found")
        }
    }
}

fn text_response(status: StatusCode, body: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(body.as_bytes())));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn resolve_target(output_dir: &Path, url: &str) -> Target {
    let path_part = url.split(['?', '#']).next().unwrap_or_default();

    let mut file_path = output_dir.to_path_buf();
    for component in path_part.split('/') {
        match component {
            "" | "." => continue,
            ".." => return Target::NotFound,
            c => file_path.push(c),
        }
    }

    if file_path.is_dir() {
        if !path_part.ends_with('/') {
            return Target::Redirect(format!("{}/", path_part));
        }
        file_path.push("index.html");
    }

    if file_path.is_file() {
        Target::File(file_path)
    } else {
        Target::NotFound
    }
}

fn content_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod test {
    use std::fs;

    use assert_fs::fixture::PathChild;
    use indoc::indoc;

    use crate::operations::Operations;
    use crate::preview::*;

    #[test]
    fn test_resolve_target() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let output = temp_dir.child("output");
        fs::create_dir_all(output.child("post")).unwrap();
        fs::write(output.child("index.html"), "home").unwrap();
        fs::write(output.child("post/index.html"), "post").unwrap();
        fs::write(output.child("style.css"), "body {}").unwrap();
        fs::write(temp_dir.child("secret.txt"), "secret").unwrap();
        let output = output.to_path_buf();

        assert_eq!(
            Target::File(output.join("index.html")),
            resolve_target(&output, "/")
        );
        assert_eq!(
            Target::File(output.join("post/index.html")),
            resolve_target(&output, "/post/")
        );
        assert_eq!(
            Target::File(output.join("post/index.html")),
            resolve_target(&output, "/post/index.html?x=1#top")
        );
        assert_eq!(
            Target::File(output.join("style.css")),
            resolve_target(&output, "/style.css")
        );
        assert_eq!(
            Target::Redirect("/post/".into()),
            resolve_target(&output, "/post")
        );
        assert_eq!(Target::NotFound, resolve_target(&output, "/missing"));
        assert_eq!(Target::NotFound, resolve_target(&output, "/../secret.txt"));
        assert_eq!(
            Target::NotFound,
            resolve_target(&output, "/post/../../secret.txt")
        );

        temp_dir.close().expect("Failed to delete temp directory")
    }

    #[test]
    fn test_content_type() {
        assert_eq!(
            "text/html; charset=utf-8",
            content_type(Path::new("a/index.html"))
        );
        assert_eq!(
            "text/css; charset=utf-8",
            content_type(Path::new("style.CSS"))
        );
        assert_eq!("image/png", content_type(Path::new("img.png")));
        assert_eq!(
            "application/octet-stream",
            content_type(Path::new("no_extension"))
        );
    }

    #[tokio::test]
    async fn test_preview_serves_published_output() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let target_dir = temp_dir.child("rgent_test");
        Operations::new(&target_dir.to_path_buf()).expect("Failed new operation");

        let content = indoc! {r#"
        title = 'Test post'
        author = 'Pawan Dubey'
        date = '2024-12-28'
        tags = []

        #####

        # Hello world"#};
        fs::write(target_dir.child("content/test.md"), content)
            .expect("Failed to write test markdown");
        Operations::publish(&target_dir, false).expect("Failed publish operation");

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind listener");
        let addr = listener
            .local_addr()
            .expect("Failed to get listener address");
        tokio::spawn(serve_connections(
            listener,
            target_dir.child("output").to_path_buf(),
        ));

        let (status, ok) = http_request(addr, "GET", "/test-post/").await;
        assert_eq!(200, status, "{}", ok);
        assert!(
            ok.contains("content-type: text/html; charset=utf-8"),
            "{}",
            ok
        );
        assert!(ok.contains("<h1>hello world</h1>"), "{}", ok);

        let (status, head) = http_request(addr, "HEAD", "/test-post/").await;
        assert_eq!(200, status, "{}", head);
        assert!(!head.contains("hello world"), "{}", head);

        let (status, redirect) = http_request(addr, "GET", "/test-post").await;
        assert_eq!(301, status, "{}", redirect);
        assert!(redirect.contains("location: /test-post/"), "{}", redirect);

        let (status, missing) = http_request(addr, "GET", "/missing").await;
        assert_eq!(404, status, "{}", missing);

        let (status, post) = http_request(addr, "POST", "/test-post/").await;
        assert_eq!(405, status, "{}", post);
        assert!(post.contains("allow: get, head"), "{}", post);

        temp_dir.close().expect("Failed to delete temp directory")
    }

    async fn http_request(addr: std::net::SocketAddr, method: &str, path: &str) -> (u16, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("Failed to connect");
        let request = format!("{} {} HTTP/1.0\r\nHost: localhost\r\n\r\n", method, path);
        stream
            .write_all(request.as_bytes())
            .await
            .expect("Failed to send request");

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("Failed to read response");

        let status = response
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or_else(|| panic!("Malformed response: {}", response));
        (status, response.to_lowercase())
    }
}
