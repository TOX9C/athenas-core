const pty = require('node-pty');
const os = require('os');
const shell = os.platform() === 'win32' ? 'powershell.exe' : '/bin/zsh';
const ptyProcess = pty.spawn(shell, [], {
  name: 'xterm-color',
  cols: 80,
  rows: 30,
  cwd: process.cwd(),
  env: process.env
});
ptyProcess.on('data', function(data) {
  process.stdout.write(data);
});
ptyProcess.write('ls\r');
setTimeout(() => { ptyProcess.kill(); }, 1000);
