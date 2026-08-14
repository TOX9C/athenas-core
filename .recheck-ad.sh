#!/bin/bash
cd .hyperframes/ad || exit 1
echo '=== hyperframes check (FULL) ==='
npx -y hyperframes check . 2>&1 | grep -vE '^npm notice' | tail -45
echo '=== hyperframes inspect ==='
npx -y hyperframes inspect . --samples 14 2>&1 | grep -vE '^npm notice' | tail -25
