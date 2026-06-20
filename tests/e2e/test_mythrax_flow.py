import pytest
import time
import subprocess
import os
import shutil
import tempfile
from mythrax_forge.client import MythraxClient
from mythrax_forge.executor import ExecutionSafetyGate, HtrExecutor
from mythrax_forge.critic import LlmCritic
from mythrax_forge.synthesis import DreamCoordinator
from mythrax_forge.compactor import RaptorCompactor
from mythrax_mcp.main import save_episode, search_memories, record_feedback

@pytest.fixture(scope="module", autouse=True)
def mock_server():
    proc = subprocess.Popen(["python3", "tests/mocks/core_server.py"])
    time.sleep(1.5)
    yield
    proc.terminate()
    proc.wait()

def test_full_agentic_flow():
    # 1. Verify Client -> Core Server interactions
    client = MythraxClient(base_url="http://127.0.0.1:8090", token="secret-token")
    
    ep_id = client.save_episode(
        title="Integration Episode",
        content="Testing connection pooling and E2E logic",
        entities=[],
        scope="e2e-testing"
    )
    assert ep_id.startswith("episode:")

    results = client.search(query="connection pooling", scope="e2e-testing", limit=1)
    assert len(results) == 1
    assert "Mock Cache" in results[0]["title"]

    # 2. Verify Execution Safety Gate AST checks
    gate = ExecutionSafetyGate()
    assert gate.is_safe("import os") is False
    assert gate.is_safe("exec('print(1)')") is False
    assert gate.is_safe("def add(a, b):\n    return a + b") is True

    # 3. Verify Sandbox Executor script running
    executor = HtrExecutor()
    res = executor.run_script("print('E2E Executor Success')")
    assert res["success"] is True
    assert "E2E Executor Success" in res["output"]

    # 4. Verify Critic Traceback parsing
    critic = LlmCritic()
    tb = """
Traceback (most recent call last):
  File "sandbox_run.py", line 2, in <module>
    raise TypeError("Invalid type")
TypeError: Invalid type
"""
    info = critic.parse_traceback(tb)
    assert info["error_type"] == "TypeError"
    assert info["line_number"] == 2
    assert "Invalid type" in info["details"]

    # 5. Verify Dreaming DBSCAN clustering
    coordinator = DreamCoordinator(client)
    episodes = [
        {"id": "ep:1", "title": "A", "content": "body A", "embedding": [0.1] * 768},
        {"id": "ep:2", "title": "B", "content": "body B", "embedding": [0.11] * 768},
        {"id": "ep:3", "title": "C", "content": "body C", "embedding": [0.9] * 768},
    ]
    clusters = coordinator.get_dream_clusters(episodes, eps=0.1, min_samples=2)
    assert len(clusters) > 0

    # 6. Verify Raptor Hierarchical Compaction
    compactor = RaptorCompactor(client)
    docs = [{"content": "Chunk A"}, {"content": "Chunk B"}]
    compaction_res = compactor.compact_hierarchical(docs)
    assert compaction_res["depth"] > 0
    assert "Combined Summary" in compaction_res["summary"]

    # 7. Verify FastMCP Tool Bridge
    mcp_ep_id = save_episode(
        title="MCP Note",
        content="Testing MCP tool mapping",
        entities=[],
        scope="e2e-testing"
    )
    assert mcp_ep_id.startswith("episode:")

    mcp_search = search_memories(query="caching", scope="e2e-testing", limit=1)
    assert len(mcp_search) == 1
    assert "Mock Cache" in mcp_search[0]["title"]

    feedback_msg = record_feedback(episode_id=mcp_ep_id, success=True)
    assert "recorded successfully" in feedback_msg

    client.close()
