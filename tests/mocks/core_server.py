from fastapi import FastAPI, Header, HTTPException
from pydantic import BaseModel
from typing import List, Optional
import uvicorn
import uuid

app = FastAPI()

class Episode(BaseModel):
    title: str
    content: str
    entities: List[dict]
    scope: Optional[str] = "general"

class SearchQuery(BaseModel):
    query: str
    scope: Optional[str] = "general"
    limit: int = 5

class Feedback(BaseModel):
    id: str
    success: bool

class ConfigLlm(BaseModel):
    provider: str
    duration: Optional[str] = "permanent"

@app.post("/v1/episodes")
async def save_episode(episode: Episode, x_mythrax_token: str = Header(None)):
    return {"status": "success", "id": f"episode:{uuid.uuid4()}"}

@app.post("/v1/search")
async def search(query: SearchQuery, x_mythrax_token: str = Header(None)):
    return [
        {
            "id": "episode:mock-123",
            "title": "Mock Cache Invalidation",
            "content": f"Mock result matching query: {query.query}",
            "similarity": 0.85,
            "utility": 1.0,
            "tier": "Standard"
        }
    ]

@app.post("/v1/feedback")
async def record_feedback(fb: Feedback, x_mythrax_token: str = Header(None)):
    return {"status": "success", "new_utility": 0.79}

@app.get("/v1/config/llm")
async def get_config_llm():
    return {
        "active_provider": "cloud",
        "cloud_provider": "gemini",
        "model": "gemini-1.5-flash",
        "is_override": False,
        "expires_at": None
    }

@app.post("/v1/config/llm")
async def update_config_llm(cfg: ConfigLlm):
    return {
        "status": "success",
        "active_provider": cfg.provider,
        "expires_at": "2026-06-20T23:59:59Z" if cfg.duration != "permanent" else None
    }

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8090)
