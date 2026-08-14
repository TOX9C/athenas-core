#!/bin/bash
cd .hyperframes/ad
npx -y hyperframes render . --quality draft --output /tmp/one-window-draft.mp4 2>&1 | grep -vE '^npm notice' | tail -12
echo '=== output ==='
ls -la /tmp/one-window-draft.mp4 2>/dev/null
