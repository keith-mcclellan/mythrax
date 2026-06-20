from mcp.server.fastmcp import FastMCP
import httpx
import os
from typing import List, Dict, Any, Optional

mcp = FastMCP("mythrax")

# Configuration
BASE_URL = os.environ.get("MYTHRAX_API_URL", "http://127.0.0.1:8090")
token_path = os.path.expanduser("~/.mythrax/token")
if os.path.exists(token_path):
    try:
        with open(token_path, "r") as f:
            TOKEN = f.read().strip()
    except Exception:
        TOKEN = "secret-token"
else:
    TOKEN = "secret-token"

def get_client():
    return httpx.Client(
        base_url=BASE_URL,
        headers={"X-Mythrax-Token": TOKEN},
        timeout=15.0
    )

@mcp.tool()
def save_episode(title: str, content: str, entities: List[Dict[str, Any]], scope: Optional[str] = "general", vault_path: Optional[str] = None) -> str:
    """Save an episode note to the Mythrax memory vault and index it in the SurrealDB cache."""
    with get_client() as client:
        payload = {
            "title": title,
            "content": content,
            "entities": entities,
            "scope": scope,
            "vault_path": vault_path
        }
        res = client.post("/v1/episodes", json=payload)
        res.raise_for_status()
        return res.json()["id"]

@mcp.tool()
def search_memories(query: str, scope: Optional[str] = "general", limit: int = 5) -> List[Dict[str, Any]]:
    """Execute a semantic vector search query over the saved episodes and memories."""
    with get_client() as client:
        payload = {
            "query": query,
            "scope": scope,
            "limit": limit
        }
        res = client.post("/v1/search", json=payload)
        res.raise_for_status()
        return res.json()

@mcp.tool()
def record_feedback(episode_id: str, success: bool) -> str:
    """Record execution feedback (reinforcement learning utility adjustment) for a saved memory."""
    with get_client() as client:
        payload = {
            "id": episode_id,
            "success": success
        }
        res = client.post("/v1/feedback", json=payload)
        res.raise_for_status()
        return "Feedback recorded successfully"

@mcp.tool()
def get_llm_config() -> Dict[str, Any]:
    """Retrieve the current LLM provider configurations."""
    with get_client() as client:
        res = client.get("/v1/config/llm")
        res.raise_for_status()
        return res.json()

@mcp.tool()
def update_llm_config(provider: str, duration: Optional[str] = "permanent") -> Dict[str, Any]:
    """Update the LLM provider configurations, either permanently or temporarily."""
    with get_client() as client:
        payload = {
            "provider": provider,
            "duration": duration
        }
        res = client.post("/v1/config/llm", json=payload)
        res.raise_for_status()
        return res.json()

if __name__ == "__main__":
    mcp.run()
