import re
from typing import Optional, Any

class LlmCritic:
    def __init__(self, client: Optional[Any] = None):
        # Placeholder client for LLM evaluation requests
        self.client = client

    def parse_traceback(self, error_output: str) -> dict:
        """Parses error traceback stack tracing to isolate the failing line and error name."""
        if not error_output:
            return {"error_type": "NoError", "line_number": None, "details": ""}

        # Simple regex matcher for Python traceback format
        lines = error_output.splitlines()
        error_type = "UnknownError"
        line_number = None
        details = ""

        # Extract last line for error details (e.g. ValueError: index out of bounds)
        for line in reversed(lines):
            if line.strip() and ":" in line:
                parts = line.split(":", 1)
                if " " not in parts[0]:
                    error_type = parts[0].strip()
                    details = parts[1].strip()
                    break

        # Search for line number
        for line in reversed(lines):
            match = re.search(r'File ".*?", line (\d+)', line)
            if match:
                line_number = int(match.group(1))
                break

        return {
            "error_type": error_type,
            "line_number": line_number,
            "details": details
        }

    def generate_critic_prompt(self, code: str, error_info: dict) -> str:
        return f"""[CRITIC_PROMPT]
Review the following Python code that failed with an error.

Code:
{code}

Error Info:
Type: {error_info.get("error_type")}
Line: {error_info.get("line_number")}
Details: {error_info.get("details")}

Analyze the root cause and provide a brief remedy.
"""
