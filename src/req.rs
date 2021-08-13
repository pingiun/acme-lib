use lazy_static::lazy_static;
use std::time::Duration;

use crate::api::ApiProblem;

pub(crate) type ReqResult<T> = std::result::Result<T, ApiProblem>;

lazy_static! {
    static ref AGENT: ureq::Agent = req_configure(ureq::AgentBuilder::new()).build();
}

pub(crate) fn req_get(url: &str) -> Result<ureq::Response, ureq::Error> {
    let req = AGENT.get(url);
    trace!("{:?}", req);
    req.call()
}

pub(crate) fn req_head(url: &str) -> Result<ureq::Response, ureq::Error> {
    let req = AGENT.head(url);
    trace!("{:?}", req);
    req.call()
}

pub(crate) fn req_post(url: &str, body: &str) -> Result<ureq::Response, ureq::Error> {
    let req = AGENT.post(url);
    let req = req.set("content-type", "application/jose+json");
    trace!("{:?} {}", req, body);
    req.send_string(body)
}

fn req_configure(agent: ureq::AgentBuilder) -> ureq::AgentBuilder {
    agent
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(30))
}

pub(crate) fn req_handle_error(
    res: Result<ureq::Response, ureq::Error>,
) -> ReqResult<ureq::Response> {
    // ok responses pass through
    if let Ok(res) = res {
        return Ok(res);
    }

    match res {
        Ok(res) => Ok(res),
        Err(ureq::Error::Status(status, res)) => {
            Err(if res.content_type() == "application/problem+json" {
                // if we were sent a problem+json, deserialize it
                let body = req_safe_read_body(res);
                serde_json::from_str(&body).unwrap_or_else(|e| ApiProblem {
                    _type: "problemJsonFail".into(),
                    detail: Some(format!(
                        "Failed to deserialize application/problem+json ({}) body: {}",
                        e.to_string(),
                        body
                    )),
                    subproblems: None,
                })
            } else {
                // some other problem
                let status = format!("{} {}", status, res.status_text());
                let body = req_safe_read_body(res);
                let detail = format!("{} body: {}", status, body);
                ApiProblem {
                    _type: "httpReqError".into(),
                    detail: Some(detail),
                    subproblems: None,
                }
            })
        }
        Err(ureq::Error::Transport(transport)) => Err(ApiProblem {
            _type: "httpReqError".into(),
            detail: Some(transport.to_string()),
            subproblems: None,
        }),
    }
}

pub(crate) fn req_extract_res(
    res: &Result<ureq::Response, ureq::Error>,
) -> Option<&ureq::Response> {
    match res {
        Ok(res) => Some(res),
        Err(ureq::Error::Status(_status, res)) => Some(res),
        Err(ureq::Error::Transport(_)) => None,
    }
}

pub(crate) fn req_expect_header(res: &ureq::Response, name: &str) -> ReqResult<String> {
    res.header(name)
        .map(|v| v.to_string())
        .ok_or_else(|| ApiProblem {
            _type: format!("Missing header: {}", name),
            detail: None,
            subproblems: None,
        })
}

pub(crate) fn req_safe_read_body(res: ureq::Response) -> String {
    use std::io::Read;
    let mut res_body = String::new();
    let mut read = res.into_reader();
    // letsencrypt sometimes closes the TLS abruptly causing io error
    // even though we did capture the body.
    read.read_to_string(&mut res_body).ok();
    res_body
}
