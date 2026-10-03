#!/usr/bin/env node
const net = require('net');

function parsePortEnv(value, name, fallback) {
  if (value === undefined) return fallback;
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    console.error(`MCP Proxy: invalid port in ${name}: ${JSON.stringify(value)} (expected 1-65535)`);
    process.exit(2);
  }
  return port;
}

// NOTE: this is the TCP port of the MCP server the proxy bridges to —
// distinct from cli.ts's ATHENA_MCP_PORT, which is its WebSocket port.
const port = parsePortEnv(process.env.ATHENA_MCP_TCP_PORT, 'ATHENA_MCP_TCP_PORT', 4545);
const host = process.env.ATHENA_MCP_HOST || '127.0.0.1';

// Track in-flight JSON-RPC requests (stdio -> TCP) so we can distinguish a
// clean shutdown from the server dying with responses still owed to us.
let pendingRequests = 0;

const client = net.createConnection({ port, host }, () => {
  process.stdin.on('data', (chunk) => {
    for (const line of chunk.toString().split('\n')) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        const msg = JSON.parse(trimmed);
        if (msg && msg.method && msg.id !== undefined) pendingRequests++;
        else if (msg && msg.id !== undefined && ('result' in msg || 'error' in msg)) pendingRequests--;
      } catch {
        // Non-JSON line (shouldn't happen for MCP stdio) — forward as-is.
      }
    }
    client.write(chunk);
  });

  client.on('data', (chunk) => {
    for (const line of chunk.toString().split('\n')) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        const msg = JSON.parse(trimmed);
        if (msg && msg.id !== undefined && ('result' in msg || 'error' in msg)) pendingRequests--;
      } catch {
        // ignore
      }
    }
    process.stdout.write(chunk);
  });

  // Half-close: our stdin is done, so tell the server we're done sending
  // without dropping still-pending responses flowing back to stdout.
  process.stdin.on('end', () => {
    client.end();
  });
});

client.on('error', (err) => {
  console.error('MCP Proxy connection error:', err.message);
  process.exit(1);
});

client.on('end', () => {
  if (pendingRequests > 0) {
    console.error(`MCP Proxy: server closed with ${pendingRequests} pending request(s) unanswered`);
    process.exit(1);
  }
  process.exit(0);
});
