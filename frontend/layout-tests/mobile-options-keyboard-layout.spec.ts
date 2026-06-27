import { test, expect, type Locator } from '@playwright/test';

const EPSILON = 1.5;

async function box(locator: Locator) {
  await expect(locator).toBeVisible();
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('Expected locator to have a visible bounding box');
  return rect;
}

test.describe('issue #36 — mobile controls and keyboard layout', () => {
  test.use({ locale: 'zh-CN' });

  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.setViewportSize({ width: 363, height: 777 });
    await expect(page.getByLabel('线程数:')).toBeVisible();
  });

  test('numeric solve inputs stay aligned on a narrow mobile viewport', async ({ page }) => {
    const solveOptions = await box(page.locator('.solve-options'));
    const threadsInput = await box(page.locator('#threads-input'));
    const topNInput = await box(page.locator('#topn-input'));

    expect(Math.abs(threadsInput.x - topNInput.x)).toBeLessThan(EPSILON);
    expect(Math.abs(threadsInput.width - topNInput.width)).toBeLessThan(EPSILON);

    for (const control of await page.locator('.option-control').all()) {
      const controlBox = await box(control);
      expect(controlBox.x).toBeGreaterThanOrEqual(solveOptions.x - EPSILON);
      expect(controlBox.x + controlBox.width).toBeLessThanOrEqual(
        solveOptions.x + solveOptions.width + EPSILON,
      );
    }
  });

  test('virtual keyboard rows do not wrap or overflow on a narrow mobile viewport', async ({
    page,
  }) => {
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
