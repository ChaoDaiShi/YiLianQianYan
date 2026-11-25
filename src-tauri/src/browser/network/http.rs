use super::policy::{checked_url, destination, permitted};
use super::{Context, BODY_LIMIT, HEADER_LIMIT, HEADER_TIMEOUT};
use std::time::Duration;
use tauri::Url;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
pub(super) struct Request {
    method: String,
    url: Url,
    pub(super) head: Vec<(String, String)>,
    length: Option<usize>,
    chunked: bool,
    websocket: bool,
}
pub(super) fn parse_request(head: &[u8]) -> Result<Request, String> {
    let text = std::str::from_utf8(head).map_err(|_| "Invalid proxy headers")?;
    let mut lines = text.split("\r\n");
    let first = lines.next().ok_or("Missing proxy request")?;
    let parts: Vec<_> = first.split(' ').collect();
    if parts.len() != 3
        || parts[2] != "HTTP/1.1"
        || !parts[0].bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err("Invalid proxy request line".into());
    }
    let method = parts[0].to_string();
    let url = if method == "CONNECT" {
        if parts[1].contains(['/', '?', '#', '@']) {
            return Err("Invalid tunnel destination".into());
        }
        checked_url(&format!("https://{}/", parts[1]))?
    } else {
        checked_url(parts[1])?
    };
    if method != "CONNECT" && url.scheme() != "http" {
        return Err("HTTPS requires a CONNECT tunnel".into());
    }
    let mut request = Request {
        method,
        url,
        head: Vec::new(),
        length: None,
        chunked: false,
        websocket: false,
    };
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("Invalid proxy header")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.chars().any(|c| c.is_control() && c != '\t')
        {
            return Err("Invalid proxy header".into());
        }
        let name = name.to_ascii_lowercase();
        let value = value.trim().to_string();
        if name == "content-length" {
            if request.length.is_some() {
                return Err("Duplicate request length".into());
            }
            let length = value
                .parse::<usize>()
                .map_err(|_| "Invalid request length")?;
            if length > BODY_LIMIT {
                return Err("Browser request body is too large".into());
            }
            request.length = Some(length);
        }
        if name == "transfer-encoding" {
            if request.chunked || !value.eq_ignore_ascii_case("chunked") {
                return Err("Unsupported request framing".into());
            }
            request.chunked = true;
        }
        if name == "upgrade" {
            if !value.eq_ignore_ascii_case("websocket") {
                return Err("Unsupported upgrade".into());
            }
            request.websocket = true;
        }
        if name == "expect" {
            return Err("Unsupported request expectation".into());
        }
        if name == "host"
            || name == "connection"
            || name == "proxy-connection"
            || name == "proxy-authorization"
            || name.starts_with("x-yilian-")
        {
            continue;
        }
        request.head.push((name, value));
    }
    if request.chunked && request.length.is_some() {
        return Err("Ambiguous request body".into());
    }
    if (request.websocket || request.method == "CONNECT")
        && (request.chunked || request.length.unwrap_or(0) > 0)
    {
        return Err("Unexpected tunnel body".into());
    }
    Ok(request)
}

pub(super) async fn read_head(client: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut next = [0u8; 1];
    while bytes.len() < HEADER_LIMIT {
        if client
            .read(&mut next)
            .await
            .map_err(|_| "Proxy request read failed")?
            == 0
        {
            return Err("Incomplete proxy request".into());
        }
        bytes.push(next[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
    Err("Proxy headers too large".into())
}

pub(super) async fn serve(mut client: TcpStream, mut context: Context) {
    let mut committed = false;
    let result = async {
        let head=tokio::select! {
            biased;
            _=context.stop.changed()=>return Err("Browser session closed".to_string()),
            result=timeout(HEADER_TIMEOUT,read_head(&mut client))=>result.map_err(|_| "Proxy headers timed out")??,
        };
        let request=parse_request(&head)?;
        let url=request.url.clone();
        let execution_context=context.clone();
        let operation=forward(&mut client,request,&execution_context,&mut committed);
        tokio::pin!(operation);
        loop {
            tokio::select! {
                biased;
                _=context.stop.changed()=>return Err("Browser session closed".to_string()),
                result=context.changed.changed()=>{
                    if result.is_err() || !context.policy.read().map(|p| permitted(&p,&url)).unwrap_or(false) {
                        return Err("Browser authorization revoked".to_string());
                    }
                },
                result=&mut operation=>return result,
            }
        }
    }.await;
    if result.is_err() && !committed {
        let _=timeout(Duration::from_secs(1),client.write_all(b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 35\r\nContent-Type: text/plain\r\n\r\nBrowser destination denied or failed")).await;
    }
    let _ = client.shutdown().await;
}

async fn forward(
    client: &mut TcpStream,
    request: Request,
    context: &Context,
    committed: &mut bool,
) -> Result<(), String> {
    let mut upstream = destination(&request.url, context).await?;
    *committed = true;
    if request.method == "CONNECT" {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .map_err(|_| "Tunnel response failed")?;
    } else {
        let url = &request.url;
        let mut path = url.path().to_string();
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        let authority = match url.port() {
            Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
            None => url.host_str().unwrap_or_default().to_string(),
        };
        let mut headers = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: {}\r\n",
            request.method,
            path,
            authority,
            if request.websocket {
                "Upgrade"
            } else {
                "close"
            }
        );
        for (name, value) in &request.head {
            headers.push_str(&format!("{name}: {value}\r\n"));
        }
        headers.push_str("\r\n");
        upstream
            .write_all(headers.as_bytes())
            .await
            .map_err(|_| "Proxy forwarding failed")?;
        if request.chunked {
            relay_chunked(client, &mut upstream).await?;
        } else if let Some(length) = request.length {
            let copied = timeout(
                Duration::from_secs(30),
                tokio::io::copy(&mut (&mut *client).take(length as u64), &mut upstream),
            )
            .await
            .map_err(|_| "Browser body timed out")?
            .map_err(|_| "Browser body failed")?;
            if copied != length as u64 {
                return Err("Incomplete browser body".into());
            }
        }
    }
    if request.method == "CONNECT" || request.websocket {
        tokio::io::copy_bidirectional(client, &mut upstream)
            .await
            .map_err(|_| "Browser tunnel interrupted")?;
    } else {
        tokio::io::copy(&mut upstream, client)
            .await
            .map_err(|_| "Browser response interrupted")?;
    }
    Ok(())
}

async fn relay_chunked(client: &mut TcpStream, upstream: &mut TcpStream) -> Result<(), String> {
    let mut total = 0usize;
    loop {
        let mut line = Vec::new();
        while !line.ends_with(b"\r\n") {
            if line.len() > 100 {
                return Err("Chunk header too large".into());
            }
            line.push(client.read_u8().await.map_err(|_| "Incomplete chunk")?);
        }
        let size = usize::from_str_radix(
            std::str::from_utf8(&line[..line.len() - 2])
                .map_err(|_| "Invalid chunk")?
                .split(';')
                .next()
                .unwrap_or(""),
            16,
        )
        .map_err(|_| "Invalid chunk")?;
        total = total.checked_add(size).ok_or("Body size overflow")?;
        if total > BODY_LIMIT {
            return Err("Browser body too large".into());
        }
        upstream
            .write_all(&line)
            .await
            .map_err(|_| "Chunk forwarding failed")?;
        if size == 0 {
            let mut end = [0u8; 2];
            client
                .read_exact(&mut end)
                .await
                .map_err(|_| "Incomplete chunk terminator")?;
            if end != *b"\r\n" {
                return Err("Chunk trailers are unsupported".into());
            }
            upstream
                .write_all(&end)
                .await
                .map_err(|_| "Chunk forwarding failed")?;
            break;
        }
        let copied = tokio::io::copy(&mut (&mut *client).take(size as u64), upstream)
            .await
            .map_err(|_| "Chunk forwarding failed")?;
        let mut end = [0u8; 2];
        client
            .read_exact(&mut end)
            .await
            .map_err(|_| "Incomplete chunk")?;
        if copied != size as u64 || end != *b"\r\n" {
            return Err("Invalid chunk body".into());
        }
        upstream
            .write_all(&end)
            .await
            .map_err(|_| "Chunk forwarding failed")?;
    }
    Ok(())
}
