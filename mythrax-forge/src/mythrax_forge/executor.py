import ast
from typing import List, Set, Optional

class ExecutionSafetyGate:
    def __init__(self, blocked_modules: Optional[Set[str]] = None):
        if blocked_modules is None:
            self.blocked_modules = {"os", "subprocess", "shutil", "socket", "requests", "urllib", "ctypes"}
        else:
            self.blocked_modules = blocked_modules

    def is_safe(self, code: str) -> bool:
        try:
            tree = ast.parse(code)
        except SyntaxError:
            return False

        for node in ast.walk(tree):
            # Check for imports
            if isinstance(node, ast.Import):
                for alias in node.names:
                    name = alias.name.split('.')[0]
                    if name in self.blocked_modules:
                        return False
            elif isinstance(node, ast.ImportFrom):
                if node.module:
                    name = node.module.split('.')[0]
                    if name in self.blocked_modules:
                        return False
            
            # Check for generic calling of builtins or dangerous actions
            if isinstance(node, ast.Call):
                # E.g. eval() or exec()
                if isinstance(node.func, ast.Name):
                    if node.func.id in {"eval", "exec", "open", "compile", "__import__"}:
                        return False
                # E.g. getattr(os, "system") or similar
                elif isinstance(node.func, ast.Attribute):
                    pass

        return True


class HtrExecutor:
    def __init__(self, sandbox_root: Optional[str] = None):
        import tempfile
        self.sandbox_root = sandbox_root or tempfile.gettempdir()

    def run_script(self, code: str, hitl_callback: Optional[callable] = None) -> dict:
        import os
        import tempfile
        import subprocess

        # 1. AST Safety Screening
        gate = ExecutionSafetyGate()
        if not gate.is_safe(code):
            return {
                "success": False,
                "output": "",
                "error": "SecurityException: Script violates execution safety gates",
                "exit_code": -1
            }

        # 2. HITL Approval Hook
        if hitl_callback is not None:
            approved = hitl_callback(code)
            if not approved:
                return {
                    "success": False,
                    "output": "",
                    "error": "HITLApprovalException: Script rejected by Human-in-the-Loop gate",
                    "exit_code": -2
                }

        # 3. Execution in isolated temp directory
        with tempfile.TemporaryDirectory(dir=self.sandbox_root) as tmpdir:
            script_path = os.path.join(tmpdir, "sandbox_run.py")
            with open(script_path, "w") as f:
                f.write(code)

            try:
                res = subprocess.run(
                    ["python3", script_path],
                    capture_output=True,
                    text=True,
                    timeout=5.0
                )
                return {
                    "success": res.returncode == 0,
                    "output": res.stdout,
                    "error": res.stderr,
                    "exit_code": res.returncode
                }
            except subprocess.TimeoutExpired as e:
                return {
                    "success": False,
                    "output": e.stdout or "",
                    "error": f"TimeoutExpired: Execution exceeded limit. {e.stderr or ''}",
                    "exit_code": -9
                }
            except Exception as e:
                return {
                    "success": False,
                    "output": "",
                    "error": str(e),
                    "exit_code": -99
                }
