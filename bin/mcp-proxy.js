#!/usr/bin/env node
const net = require('net');

const client = net.createConnection({ port: 4545 }, () => {
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
