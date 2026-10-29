use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::Client;
use std::time::Duration;

pub struct StealthHttpClient {
    client: Client,
    user_agent: String,
}

impl StealthHttpClient {
    pub fn new(custom_ua: Option<&str>) -> Result<Self, reqwest::Error> {
        let ua = custom_ua.unwrap_or(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
        );

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36"
        ));
        headers.insert("sec-ch-ua", HeaderValue::from_static(
            r#""Chromium";v="128", "Not;A=Brand";v="24", "Google Chrome";v="128""#
        ));
        headers.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
        headers.insert("sec-ch-ua-platform", HeaderValue::from_static(r#""Windows""#));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .build()?;

        Ok(Self {
            client,
            user_agent: ua.to_string(),
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}
