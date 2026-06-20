import pytest
from mythrax_forge.executor import HtrExecutor

def test_executor_success():
    executor = HtrExecutor()
    code = "print('Hello Output')"
    res = executor.run_script(code)
    assert res["success"] is True
    assert "Hello Output" in res["output"]
    assert res["exit_code"] == 0

def test_executor_gate_violation():
    executor = HtrExecutor()
    code = "import os\nos.system('echo')"
    res = executor.run_script(code)
    assert res["success"] is False
    assert "SecurityException" in res["error"]
    assert res["exit_code"] == -1

def test_executor_hitl_rejection():
    executor = HtrExecutor()
    code = "x = 1"
    res = executor.run_script(code, hitl_callback=lambda c: False)
    assert res["success"] is False
    assert "HITLApprovalException" in res["error"]
    assert res["exit_code"] == -2
