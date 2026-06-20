import pytest
from mythrax_forge.compactor import RaptorCompactor

def test_raptor_compaction():
    compactor = RaptorCompactor(None)
    episodes = [
        {"content": "First details"},
        {"content": "Second details"},
        {"content": "Third details"},
    ]
    res = compactor.compact_hierarchical(episodes)
    assert res["depth"] > 1
    assert "Combined Summary" in res["summary"]
