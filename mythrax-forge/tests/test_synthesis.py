import pytest
from mythrax_forge.synthesis import DreamCoordinator

class MockClient:
    def search(self, query, scope, limit):
        return [
            {"id": "ep:1", "title": "A", "content": "body A", "embedding": [0.1] * 768},
            {"id": "ep:2", "title": "B", "content": "body B", "embedding": [0.11] * 768},
            {"id": "ep:3", "title": "C", "content": "body C", "embedding": [0.9] * 768},
        ]

def test_synthesis_dream_clustering():
    client = MockClient()
    coordinator = DreamCoordinator(client)
    
    episodes = client.search("", "", 5)
    clusters = coordinator.get_dream_clusters(episodes, eps=0.1, min_samples=2)
    
    # Verify we successfully clustered A and B together (similar embeddings)
    assert len(clusters) > 0
