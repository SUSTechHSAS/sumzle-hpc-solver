import { test, expect, type Locator } from '@playwright/test';

const EPSILON = 1.5;

async function box(locator: Locator) {
  await expect(locator).toBeVisible();
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('Expected locator to have a visible bounding box');
  return rect;
}

test.describe('issue #36 — mobile controls and keyboard layout', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await expect(page.getByLabel('线程数:')).toBeVisible();
  });

  for (const viewport of [
    { name: 'desktop', width: 1280, height: 900 },
    { name: 'mobile', width: 363, height: 777 },
  ]) {
    test(`solve option columns stay aligned on ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });

      const solveOptions = await box(page.locator('.solve-options'));
      const labels = await Promise.all(
        [
          page.locator('label[for="threads-input"]'),
          page.locator('label[for="topn-input"]'),
          page.locator('label[for="progress-toggle"]'),
        ].map((locator) => box(locator)),
      );
      const threadsInput = await box(page.locator('#threads-input'));
      const topNInput = await box(page.locator('#topn-input'));
      const progressToggle = await box(page.locator('#progress-toggle'));
      const hints = await Promise.all(
        (await page.locator('.option-control .option-hint').all()).map(box),
      );

      for (const label of labels.slice(1)) {
        expect(Math.abs(label.x + label.width - (labels[0].x + labels[0].width))).toBeLessThan(
          EPSILON,
        );
      }

      expect(Math.abs(threadsInput.x - topNInput.x)).toBeLessThan(EPSILON);
      expect(Math.abs(threadsInput.width - topNInput.width)).toBeLessThan(EPSILON);
      expect(progressToggle.x).toBeGreaterThanOrEqual(threadsInput.x - EPSILON);
      expect(progressToggle.x + progressToggle.width).toBeLessThanOrEqual(
        threadsInput.x + threadsInput.width + EPSILON,
      );

      for (const hint of hints.slice(1)) {
        expect(Math.abs(hint.x - hints[0].x)).toBeLessThan(EPSILON);
      }

      const visibleOptionParts = page.locator(
        '.option-control label, .option-control input, .option-control .option-hint',
      );
      for (const part of await visibleOptionParts.all()) {
        const partBox = await box(part);
        expect(partBox.x).toBeGreaterThanOrEqual(solveOptions.x - EPSILON);
        expect(partBox.x + partBox.width).toBeLessThanOrEqual(
          solveOptions.x + solveOptions.width + EPSILON,
        );
      }
    });
  }

  test('virtual keyboard rows do not wrap or overflow on a narrow mobile viewport', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 363, height: 777 });

    const rows = await page.locator('.keyboard-row').all();
    expect(rows.length).toBeGreaterThan(0);

    for (const row of rows) {
      const rowBox = await box(row);
      const keys = await row.locator('.keyboard-key').all();
      expect(keys.length).toBeGreaterThan(0);

      const firstKeyBox = await box(keys[0]);
      for (const key of keys) {
        const keyBox = await box(key);
        expect(Math.abs(keyBox.y - firstKeyBox.y)).toBeLessThan(EPSILON);
        expect(keyBox.x).toBeGreaterThanOrEqual(rowBox.x - EPSILON);
        expect(keyBox.x + keyBox.width).toBeLessThanOrEqual(rowBox.x + rowBox.width + EPSILON);
      }
    }
  });
});
