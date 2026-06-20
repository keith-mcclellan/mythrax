from mythrax_forge.executor import ExecutionSafetyGate

def test_safety_gate_allowed():
    gate = ExecutionSafetyGate()
    code = """
def add(a, b):
    return a + b
print(add(2, 3))
"""
    assert gate.is_safe(code) is True

def test_safety_gate_blocked_import():
    gate = ExecutionSafetyGate()
    code = """
import os
os.system("echo 'hack'")
"""
    assert gate.is_safe(code) is False

def test_safety_gate_blocked_import_from():
    gate = ExecutionSafetyGate()
    code = """
from subprocess import Popen
"""
    assert gate.is_safe(code) is False

def test_safety_gate_blocked_builtins():
    gate = ExecutionSafetyGate()
    code = """
exec("import os")
"""
    assert gate.is_safe(code) is False
