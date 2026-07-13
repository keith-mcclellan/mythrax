#!/bin/bash
set -e
# Execute the python verification script directly
python3 "$(dirname "$0")/verify_dev25.py"
