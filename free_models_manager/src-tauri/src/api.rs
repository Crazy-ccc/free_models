use rand::Rng;
use reqwest::{Client, Method};
use serde_json::Value;
use sha2::{Digest, Sha256};
use crate::crypto::KeyPair;

pub struct AdminClient {
    client: Client,
    server_url: String,
    keypair: KeyPair,
}

impl AdminClient {
    pub fn new(server_url: String, keypair: KeyPair) -> Self {
        AdminClient { client: Client::new(), server_url, keypair }
    }

    async fn signed_request(&self, method: &str, path: &str, body: Option<String>) -> Result<reqwest::Response, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();

        // 生成随机 nonce（16 个 hex 字符 = 8 字节）
        let mut nonce_bytes = [0u8; 8];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = nonce_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

        // 计算 SHA-256 body hash
        let body_hash = match &body {
            Some(b) => {
                let mut hasher = Sha256::new();
                hasher.update(b.as_bytes());
                let hash = hasher.finalize();
                hash.iter().map(|b| format!("{:02x}", b)).collect::<String>()
            }
            None => String::new(),
        };

        // 签名只使用纯路径（不含 query string），服务端用 req.path() 验证同样不含 query
        let sign_path = path.split('?').next().unwrap_or(path);
        let payload = format!("{}:{}:{}:{}:{}", method, sign_path, timestamp, nonce, body_hash);
        let signature = self.keypair.sign(&payload);

        let url = format!("{}{}", self.server_url, path);
        let method = match method {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            _ => return Err("Invalid method".to_string()),
        };
        let mut req = self.client.request(method, &url);
        req = req.header("X-Admin-Fingerprint", &self.keypair.fingerprint);
        req = req.header("X-Admin-Timestamp", &timestamp);
        req = req.header("X-Admin-Nonce", &nonce);
        req = req.header("X-Admin-Signature", &signature);
        if !body_hash.is_empty() {
            req = req.header("X-Admin-Body-Hash", &body_hash);
        }

        if let Some(body) = body {
            req = req.header("Content-Type", "application/json");
            req = req.body(body);
        }

        req.send().await.map_err(|e| e.to_string())
    }

    pub async fn get_service_status(&self) -> Result<Value, String> {
        let resp = self.signed_request("GET", "/admin/service/status", None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn refresh_cache(&self) -> Result<(), String> {
        let resp = self.signed_request("POST", "/admin/cache/refresh", None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(())
    }

    pub async fn list_providers(&self) -> Result<Vec<Value>, String> {
        let resp = self.signed_request("GET", "/admin/providers", None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn create_provider(&self, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", "/admin/providers", Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_provider(&self, id: i32) -> Result<Value, String> {
        let resp = self.signed_request("GET", &format!("/admin/providers/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn update_provider(&self, id: i32, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("PUT", &format!("/admin/providers/{}", id), Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn delete_provider(&self, id: i32) -> Result<(), String> {
        let resp = self.signed_request("DELETE", &format!("/admin/providers/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(())
    }

    pub async fn list_provider_models(&self, provider_id: i32) -> Result<Vec<Value>, String> {
        let resp = self.signed_request("GET", &format!("/admin/providers/{}/models", provider_id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn list_models(&self) -> Result<Vec<Value>, String> {
        let resp = self.signed_request("GET", "/admin/models", None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn create_model(&self, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", "/admin/models", Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_model(&self, id: i32) -> Result<Value, String> {
        let resp = self.signed_request("GET", &format!("/admin/models/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn update_model(&self, id: i32, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("PUT", &format!("/admin/models/{}", id), Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn delete_model(&self, id: i32) -> Result<(), String> {
        let resp = self.signed_request("DELETE", &format!("/admin/models/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(())
    }

    pub async fn list_api_keys(&self) -> Result<Vec<Value>, String> {
        let resp = self.signed_request("GET", "/admin/api_keys", None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn create_api_key(&self, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", "/admin/api_keys", Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_api_key(&self, id: i32) -> Result<Value, String> {
        let resp = self.signed_request("GET", &format!("/admin/api_keys/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn update_api_key(&self, id: i32, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("PUT", &format!("/admin/api_keys/{}", id), Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn delete_api_key(&self, id: i32) -> Result<(), String> {
        let resp = self.signed_request("DELETE", &format!("/admin/api_keys/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(())
    }

    pub async fn list_provider_credentials(&self, provider_id: Option<i32>) -> Result<Vec<Value>, String> {
        let path = match provider_id {
            Some(pid) => format!("/admin/provider_credentials?provider_id={}", pid),
            None => "/admin/provider_credentials".to_string(),
        };
        let resp = self.signed_request("GET", &path, None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn create_provider_credential(&self, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", "/admin/provider_credentials", Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_provider_credential(&self, id: i32) -> Result<Value, String> {
        let resp = self.signed_request("GET", &format!("/admin/provider_credentials/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn update_provider_credential(&self, id: i32, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("PUT", &format!("/admin/provider_credentials/{}", id), Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn delete_provider_credential(&self, id: i32) -> Result<(), String> {
        let resp = self.signed_request("DELETE", &format!("/admin/provider_credentials/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(())
    }

    pub async fn reset_provider_credential_status(&self, id: i32) -> Result<Value, String> {
        let resp = self.signed_request("POST", &format!("/admin/provider_credentials/{}/reset_status", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn test_provider_credential(&self, credential_id: i32, model_id: String, prompt: Option<String>) -> Result<Value, String> {
        let body = serde_json::json!({
            "credential_id": credential_id,
            "model_id": model_id,
            "prompt": prompt,
        });
        let body = serde_json::to_string(&body).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", "/admin/test_credential", Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn list_provider_model_maps(&self, model_id: Option<i32>, provider_id: Option<i32>) -> Result<Vec<Value>, String> {
        let mut path = "/admin/provider_model_maps".to_string();
        let mut params = Vec::new();
        if let Some(mid) = model_id {
            params.push(format!("model_id={}", mid));
        }
        if let Some(pid) = provider_id {
            params.push(format!("provider_id={}", pid));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        let resp = self.signed_request("GET", &path, None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn create_provider_model_map(&self, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", "/admin/provider_model_maps", Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn update_provider_model_map(&self, id: i32, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("PUT", &format!("/admin/provider_model_maps/{}", id), Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn delete_provider_model_map(&self, id: i32) -> Result<(), String> {
        let resp = self.signed_request("DELETE", &format!("/admin/provider_model_maps/{}", id), None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(())
    }

    pub async fn import_provider_models(&self, provider_id: i32, data: Value) -> Result<Value, String> {
        let body = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let resp = self.signed_request("POST", &format!("/admin/providers/{}/models/import", provider_id), Some(body)).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    pub async fn usage_log_stats(
        &self,
        group_by: &str,
        start_time: Option<&str>,
        end_time: Option<&str>,
    ) -> Result<Value, String> {
        let mut path = format!("/admin/usage_log/stats?group_by={}", group_by);
        if let Some(v) = start_time {
            path.push_str(&format!("&start_time={}", v));
        }
        if let Some(v) = end_time {
            path.push_str(&format!("&end_time={}", v));
        }
        let resp = self.signed_request("GET", &path, None).await?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }
}
