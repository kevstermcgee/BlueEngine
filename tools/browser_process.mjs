// A profile cannot be removed while Chromium is still writing it.
import {rm} from 'node:fs/promises';

export async function cleanupBrowser(browser, profile, socket) {
  socket?.close();
  if (browser.pid && browser.exitCode === null && browser.signalCode === null) {
    await new Promise(resolve => {
      const force = setTimeout(() => browser.kill('SIGKILL'), 3000);
      browser.once('close', () => { clearTimeout(force); resolve(); });
      browser.kill('SIGTERM');
    });
  }
  // Chromium children can finish profile writes just after the parent exits.
  await rm(profile, {recursive: true, force: true, maxRetries: 8, retryDelay: 100});
}
