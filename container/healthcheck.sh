#!/bin/bash
set -euo pipefail
kill -0 "$(cat /run/hku-keepalive.pid)"
# Healthy means the service is running, not that interactive MFA has completed.
