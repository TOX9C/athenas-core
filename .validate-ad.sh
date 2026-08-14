#!/bin/bash
cd .hyperframes/ad || exit 1
rm -f assets/08-workspace-final.png
echo '=== assets now ==='
ls assets/
echo '=== hyperframes lint ==='
npx -y hyperframes lint . 2>&1 | tail -30
echo "lint exit: $?"
