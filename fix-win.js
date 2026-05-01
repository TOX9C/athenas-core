const fs = require('fs');
const contents = fs.readFileSync('electron/athenaOrchestrator.ts', 'utf-8');
const fixed = contents.replace(
  'private async sendAnthropic(apiKey: string, model: string, systemPrompt: string, userText: string): Promise<string> {',
  'private async sendAnthropic(apiKey: string, model: string, systemPrompt: string, userText: string): Promise<string> {\n    const win = BrowserWindow.getAllWindows()[0] ?? null;'
);
fs.writeFileSync('electron/athenaOrchestrator.ts', fixed);
