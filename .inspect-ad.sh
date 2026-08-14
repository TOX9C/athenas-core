#!/bin/bash
cd .hyperframes/ad || exit 1
echo '=== hyperframes check (contrast) ==='
npx -y hyperframes check . 2>&1 | tail -12
echo '=== hyperframes inspect ==='
npx -y hyperframes inspect . --samples 14 2>&1 | tail -30
