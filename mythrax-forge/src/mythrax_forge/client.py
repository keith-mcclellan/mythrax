import httpx
import logging
from typing import List, Optional, Dict, Any

logger = logging.getLogger("mythrax.client")

class MythraxClient:
    def __init__(self, base_url: str = "http://127.0.0.1:8090", token: str = "secret-token"):
        self.base_url = base_url
        self.headers = {"X-Mythrax-Token": token}
        # Configure connection-pooled HTTP client with HTTP/2 enabled
        self.client = httpx.Client(
            base_url=self.base_url,
            headers=self.headers,
            http2=True,
            timeout=10.0
        )

    def save_episode(self, title: str, content: str, entities: List[Dict[str, Any]], scope: Optional[str] = "general", vault_path: Optional[str] = None) -> str:
        payload = {
            "title": title,
            "content": content,
            "entities": entities,
            "scope": scope,
            "vault_path": vault_path
        }
        res = self.client.post("/v1/episodes", json=payload)
        res.raise_for_status()
        return res.json()["id"]

    def search(self, query: str, scope: Optional[str] = "general", limit: int = 5) -> List[Dict[str, Any]]:
        payload = {
            "query": query,
            "scope": scope,
            "limit": limit
        }
        res = self.client.post("/v1/search", json=payload)
        res.raise_for_status()
        return res.json()

    def record_feedback(self, episode_id: str, success: bool) -> Dict[str, Any]:
        payload = {
            "id": episode_id,
            "success": success
        }
        res = self.client.post("/v1/feedback", json=payload)
        res.raise_for_status()
        return res.json()

    def get_llm_config(self) -> Dict[str, Any]:
        res = self.client.get("/v1/config/llm")
        res.raise_for_status()
        return res.json()

    def update_llm_config(self, provider: str, duration: Optional[str] = "permanent") -> Dict[str, Any]:
        payload = {
            "provider": provider,
            "duration": duration
        }
        res = self.client.post("/v1/config/llm", json=payload)
        res.raise_for_status()
        return res.json()

    def close(self):
        self.client.close()
