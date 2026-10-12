// Replay the captured terminal bytes; no UI elements are recreated or overlaid.
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

const [recordings, output, modules, chrome, fontDir] = process.argv.slice(2);
const require = createRequire(join(modules, 'package.json'));
const { chromium } = require('playwright-core');
// Match Chromium's physical pixel scale to xterm's canvas backing store.
const browser = await chromium.launch({
  executablePath: chrome, headless: true, args: ['--force-device-scale-factor=2'],
});
const emojiCss = `@font-face {
  font-family: 'Noto Color Emoji';
  src: url(data:font/ttf;base64,${readFileSync(join(fontDir, 'NotoColorEmoji.ttf')).toString('base64')});
}`;
const fontCss = [['Regular', 400], ['Bold', 700]].map(([style, weight]) => `
  @font-face {
    font-family: 'Moralerspace Argon'; font-weight: ${weight};
    src: url(data:font/ttf;base64,${readFileSync(join(fontDir, `MoralerspaceArgon-${style}.ttf`)).toString('base64')});
  }
`).join('\n');

try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1200 }, deviceScaleFactor: 2 });
  for (const shot of JSON.parse(readFileSync(join(recordings, 'shots.json'), 'utf8'))) {
    await page.setContent(`<style>
      ${emojiCss}
      ${fontCss}
      body { margin: 0; background: #161b22; }
      #frame { display: inline-block; padding: 20px; background: #161b22; }
      .xterm-viewport { overflow: hidden !important; }
    </style><div id="frame"><div id="terminal"></div></div>`);
    await page.addStyleTag({ path: join(modules, '@xterm/xterm/css/xterm.css') });
    await page.addScriptTag({ path: join(modules, '@xterm/xterm/lib/xterm.js') });
    await page.addScriptTag({ path: join(modules, '@xterm/addon-unicode11/lib/addon-unicode11.js') });
    await page.addScriptTag({ path: join(modules, '@xterm/addon-canvas/lib/addon-canvas.js') });
    const text = await page.evaluate(async ({ shot, ansi }) => {
      await Promise.all([
        document.fonts.load('16px "Moralerspace Argon"'),
        document.fonts.load('700 16px "Moralerspace Argon"'),
        document.fonts.load('16px "Noto Color Emoji"', '✨🐛'),
      ]);
      const term = new Terminal({
        cols: shot.cols, rows: shot.rows, fontSize: 16, lineHeight: 1,
        fontFamily: '"Moralerspace Argon", "Noto Color Emoji", monospace',
        allowProposedApi: true, scrollback: 0, drawBoldTextInBrightColors: false,
        theme: {
          background: '#161b22', foreground: '#d8dee9',
          black: '#20252d', red: '#f07178', green: '#9ece6a', yellow: '#e0af68',
          blue: '#7aa2f7', magenta: '#bb9af7', cyan: '#7dcfff', white: '#c0caf5',
          brightBlack: '#78869b', brightRed: '#ff8990', brightGreen: '#b4e88d',
          brightYellow: '#f0cf8e', brightBlue: '#98b8ff', brightMagenta: '#cfb0ff',
          brightCyan: '#a4e4ff', brightWhite: '#f0f4fc',
        },
      });
      term.loadAddon(new Unicode11Addon.Unicode11Addon());
      term.unicode.activeVersion = '11';
      term.open(document.getElementById('terminal'));
      term.loadAddon(new CanvasAddon.CanvasAddon());
      await new Promise(resolve => term.write(ansi, resolve));
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      // Repaint every row after replay and canvas sizing have settled.
      term.refresh(0, shot.rows - 1);
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      for (const canvas of document.querySelectorAll('canvas')) {
        if (canvas.width !== Math.round(canvas.clientWidth * devicePixelRatio)
            || canvas.height !== Math.round(canvas.clientHeight * devicePixelRatio)) {
          throw new Error('Canvas pixel scale does not match the terminal; capture would be clipped');
        }
      }
      return Array.from({ length: shot.rows }, (_, i) => term.buffer.active.getLine(i).translateToString(true)).join('\n');
    }, { shot, ansi: readFileSync(join(recordings, `${shot.name}.ansi`), 'utf8') });
    for (const expected of shot.expected) {
      if (!text.includes(expected)) throw new Error(`${shot.name}: terminal replay missing ${expected}`);
    }
    writeFileSync(join(recordings, `${shot.name}-replayed.txt`), text);
    await page.locator('#frame').screenshot({ path: join(output, `${shot.name}.png`) });
    console.log(`${shot.name}: captured ${shot.cols} × ${shot.rows} cells`);
  }
} finally {
  await browser.close();
}
