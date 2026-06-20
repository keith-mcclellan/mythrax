from mythrax_forge.critic import LlmCritic

def test_critic_parse_no_error():
    critic = LlmCritic()
    info = critic.parse_traceback("")
    assert info["error_type"] == "NoError"

def test_critic_parse_value_error():
    critic = LlmCritic()
    error_log = """
Traceback (most recent call last):
  File "sandbox_run.py", line 4, in <module>
    raise ValueError("invalid value passed")
ValueError: invalid value passed
"""
    info = critic.parse_traceback(error_log)
    assert info["error_type"] == "ValueError"
    assert info["line_number"] == 4
    assert "invalid value passed" in info["details"]

def test_critic_prompt_gen():
    critic = LlmCritic()
    info = {"error_type": "IndexError", "line_number": 10, "details": "out of range"}
    prompt = critic.generate_critic_prompt("items[10]", info)
    assert "[CRITIC_PROMPT]" in prompt
    assert "IndexError" in prompt
