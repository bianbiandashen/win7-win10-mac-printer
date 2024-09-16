use reqwest::{Client, Error, Response};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpMethod {
    GET,
    POST,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRequest {
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub method: HttpMethod,
}

impl ApiRequest {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
            method: HttpMethod::POST,  // 默认使用POST
        }
    }

    pub fn set_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    pub fn set_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    pub fn set_method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }
}

#[tauri::command]
pub async fn send_request_command(request: ApiRequest) -> Result<String, String> {
    let client = Client::new();
    let mut request_builder = match request.method {
        HttpMethod::GET => client.get(&request.url),
        HttpMethod::POST => client.post(&request.url),
    };

    for (key, value) in request.headers {
        request_builder = request_builder.header(&key, &value);
    }

    if let Some(body) = request.body {
        request_builder = request_builder.body(body);
    }

    match request_builder.send().await {
        Ok(response) => match response.text().await {
            Ok(text) => Ok(text),
            Err(e) => Err(format!("Failed to read response text: {}", e)),
        },
        Err(e) => Err(format!("Request failed: {}", e)),
    }
}
