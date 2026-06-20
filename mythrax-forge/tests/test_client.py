import pytest
import time
import subprocess
from mythrax_forge.client import MythraxClient

@pytest.fixture(scope="module", autouse=True)
def mock_server():
    # Spawn the mock API server as a subprocess
    proc = subprocess.Popen(["python3", "tests/mocks/core_server.py"])
    time.sleep(1.5)  # Wait for uvicorn to start up
    yield
    proc.terminate()
    proc.wait()

def test_client_endpoints():
    client = MythraxClient(base_url="http://127.0.0.1:8090", token="secret-token")
    
    # Test save_episode
    ep_id = client.save_episode(
        title="Test from Client",
        content="Testing connection pooling",
        entities=[],
        scope="testing",
        vault_path="episodes/client_test.md"
    )
    assert ep_id.startswith("episode:")

    # Test search
    results = client.search(query="connection pooling", scope="testing", limit=1)
    assert len(results) == 1
    assert "Mock Cache" in results[0]["title"]

    # Test feedback
    fb_res = client.record_feedback(episode_id=ep_id, success=True)
    assert fb_res["status"] == "success"

    # Test config GET
    cfg = client.get_llm_config()
    assert cfg["active_provider"] == "cloud"

    # Test config POST
    update_res = client.update_llm_config(provider="local", duration="temporary")
    assert update_res["status"] == "success"

    client.close()
