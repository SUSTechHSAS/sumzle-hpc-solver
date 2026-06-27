import { test, expect, type Locator, type Page } from '@playwright/test';

const EPSILON = 1.5;

type Box = {
  top: number;
  left: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
  centerY: number;
};

async function fontSize(locator: Locator): Promise<number> {
  await expect(locator).toBeVisible();
  return locator.evaluate((element) => Number.parseFloat(getComputedStyle(element).fontSize));
}

async function box(locator: Locator): Promise<Box> {
  await expect(locator).toBeVisible();
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('Expected visible element to have a bounding box');
  return {
    top: rect.y,
    left: rect.x,
    right: rect.x + rect.width,
    bottom: rect.y + rect.height,
    width: rect.width,
    height: rect.height,
    centerY: rect.y + rect.height / 2,
  };
}

async function loadApp(page: Page, viewport: { width: number; height: number }) {
  await page.setViewportSize(viewport);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Sumzle Solver' })).toBeVisible();
}

async function expectInside(inner: Box, outer: Box) {
  expect(inner.left).toBeGreaterThanOrEqual(outer.left - EPSILON);
  expect(inner.right).toBeLessThanOrEqual(outer.right + EPSILON);
}

test.describe('issue #42 — progress toggle hint alignment', () => {
  test('progress hint is vertically centered with the checkbox label on desktop', async ({
    page,
  }) => {
    await loadApp(page, { width: 1280, height: 900 });

    const control = page.locator('.option-control-checkbox');
    const labelText = control.locator('.progress-toggle-text');
    const hint = control.locator('.option-hint');
    const labelBox = await box(labelText);
    const hintBox = await box(hint);
    const labelFontSize = await fontSize(labelText);
    const hintFontSize = await fontSize(hint);

    expect(Math.abs(labelBox.centerY - hintBox.centerY)).toBeLessThan(EPSILON);
    expect(hintFontSize).toBeLessThan(labelFontSize);
  });
});

test.describe('issue #36 — mobile solve options and keyboard layout', () => {
  for (const viewport of [
    { width: 363, height: 777 },
    { width: 320, height: 700 },
  ]) {
    test(`numeric solve options align and fit at ${viewport.width}px`, async ({ page }) => {
      await loadApp(page, viewport);

      const solveOptions = await box(page.locator('.solve-options'));
      const threads = await box(page.locator('#threads-input'));
      const topN = await box(page.locator('#topn-input'));

      expect(Math.abs(threads.left - topN.left)).toBeLessThan(EPSILON);
      expect(Math.abs(threads.width - topN.width)).toBeLessThan(EPSILON);
      expect(Math.abs(threads.height - topN.height)).toBeLessThan(EPSILON);
      await expectInside(threads, solveOptions);
      await expectInside(topN, solveOptions);
    });

    test(`keyboard rows stay single-line and fit at ${viewport.width}px`, async ({ page }) => {
      await loadApp(page, viewport);

      const appMain = await box(page.locator('.app-main'));
      const keyboard = await box(page.locator('.virtual-keyboard'));
      await expectInside(keyboard, appMain);

      const rows = page.locator('.keyboard-row');
      const rowCount = await rows.count();
      expect(rowCount).toBeGreaterThan(0);

      for (let rowIndex = 0; rowIndex < rowCount; rowIndex += 1) {
        const row = rows.nth(rowIndex);
        const rowBox = await box(row);
        await expectInside(rowBox, keyboard);

        const keys = row.locator('.keyboard-key');
        const keyCount = await keys.count();
        expect(keyCount).toBeGreaterThan(0);

        const firstKey = await box(keys.first());
        for (let keyIndex = 0; keyIndex < keyCount; keyIndex += 1) {
          const keyBox = await box(keys.nth(keyIndex));
          expect(Math.abs(keyBox.centerY - firstKey.centerY)).toBeLessThan(EPSILON);
          await expectInside(keyBox, rowBox);
        }
      }
    });
  }
});

test.describe('desktop layout regression coverage', () => {
  test('desktop keeps the two-column workbench and non-mobile keyboard sizing', async ({
    page,
  }) => {
    await loadApp(page, { width: 1280, height: 900 });

    const leftColumn = await box(page.locator('.column-left'));
    const rightColumn = await box(page.locator('.column-right'));
    expect(rightColumn.left).toBeGreaterThan(leftColumn.right);

    const solveOptions = await box(page.locator('.solve-options'));
    const guessRows = await box(page.locator('.guess-rows'));
    expect(solveOptions.bottom).toBeLessThanOrEqual(guessRows.top + EPSILON);
    await expectInside(solveOptions, leftColumn);

    const firstKey = await box(page.locator('.keyboard-key').first());
    expect(firstKey.width).toBeGreaterThan(32);
    expect(firstKey.height).toBeGreaterThan(38);
  });
});
