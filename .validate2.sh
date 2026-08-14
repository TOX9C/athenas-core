#!/bin/bash
cd .hyperframes/ad || exit 1
echo '=== hyperframes lint ==='
npx -y hyperframes lint . 2>&1 | tail -25
echo '=== hyperframes validate (contrast) ==='
npx -y hyperframes validate . 2>&1 | tail -25
