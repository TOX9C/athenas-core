#!/usr/bin/env node
const net = require('net');

const port = parseInt(process.env.ATHENA_MCP_PORT || '4545', 10);
const host = process.env.ATHENA_MCP_HOST || '127.0.0.1';

const client = net.createConnection({ port, host }, () => {
  process.stdin.pipe(client);
  client.pipe(process.stdout);
});

client.on('error', (err) => {
  console.error('MCP Proxy connection error:', err.message);
  process.exit(1);
});

client.on('end', () => {
  process.exit(0);
});
