import { test, expect, type Page } from '@playwright/test';

/**
 * Layout-shift regression tests for the expression-length input error.
 *
 * Background (issue #33): the inline length error message used to push the
 * three row buttons (添加行 / 删除行 / 清空) around — both horizontally
 * (wrapping onto a new line when the error text made .length-control wider
 * than the remaining space) and vertically (re-centering when
 * .length-control grew taller). The fix takes the error out of flow on
 * desktop so .length-control's box is dictated solely by the label+input
 * row, and .row-buttons never moves when an error appears or changes
 * length.
 *
 * These tests run in a real Chromium engine so they can assert on actual
 * rendered geometry (getBoundingClientRect) — something jsdom cannot do.
 * If the error ever returns to in-flow and starts pushing the buttons,
 * these tests fail immediately.
 */

/** Tolerance for sub-pixel rendering differences across platforms (font
 * rendering, high-DPI scaling, etc.). 1.0px is loose enough to avoid CI
 * flakiness on different OSes but still catches real layout shifts, which
 * typically move elements by 10px or more. */
const EPSILON = 1.0;

async function rowButtonsBox(page: Page) {
  const box = await page.locator('.row-buttons').boundingBox();
  if (!box) throw new Error('.row-buttons not found or not visible');
  return { top: box.y, left: box.x, width: box.width, height: box.height };
}

test.describe('issue #33 — length error must not shift the row buttons', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.setViewportSize({ width: 1280, height: 900 });
    // Dismiss any focus state on the input before measuring.
    await page.locator('body').click();
  });

  test('row buttons stay put when a long error appears (desktop)', async ({ page }) => {
    const input = page.getByLabel('表达式长度:');
    const before = await rowButtonsBox(page);

    // Trigger the longest error message ("…不能超过 64…").
    await input.fill('65');
    await expect(page.getByRole('alert')).toBeVisible();

    const after = await rowButtonsBox(page);
    expect(Math.abs(after.top - before.top)).toBeLessThan(EPSILON);
    expect(Math.abs(after.left - before.left)).toBeLessThan(EPSILON);
    expect(Math.abs(after.width - before.width)).toBeLessThan(EPSILON);
    expect(Math.abs(after.height - before.height)).toBeLessThan(EPSILON);
  });

  test('row buttons stay put when a short error appears (desktop)', async ({ page }) => {
    const input = page.getByLabel('表达式长度:');
    const before = await rowButtonsBox(page);

    // Trigger the short error message ("…必须是正整数。").
    await input.fill('-5');
    await expect(page.getByRole('alert')).toBeVisible();

    const after = await rowButtonsBox(page);
    expect(Math.abs(after.top - before.top)).toBeLessThan(EPSILON);
    expect(Math.abs(after.left - before.left)).toBeLessThan(EPSILON);
  });

  test('row buttons position is identical across long and short errors (desktop)', async ({
    page,
  }) => {
    const input = page.getByLabel('表达式长度:');

    await input.fill('65');
    await expect(page.getByRole('alert')).toBeVisible();
    const longErr = await rowButtonsBox(page);

    await input.fill('-5');
    await expect(page.getByRole('alert')).toBeVisible();
    const shortErr = await rowButtonsBox(page);

    // The whole point of #33: button position must not depend on error
    // message length.
    expect(Math.abs(longErr.top - shortErr.top)).toBeLessThan(EPSILON);
    expect(Math.abs(longErr.left - shortErr.left)).toBeLessThan(EPSILON);
  });

  test('clearing the error restores the exact pre-error button geometry', async ({ page }) => {
    const input = page.getByLabel('表达式长度:');
    const before = await rowButtonsBox(page);

    await input.fill('65');
    await expect(page.getByRole('alert')).toBeVisible();
    // Normalise back to a valid value (blur clamps, or just type a valid one).
    await input.fill('5');
    await expect(page.getByRole('alert')).toHaveCount(0);

    const restored = await rowButtonsBox(page);
    expect(Math.abs(restored.top - before.top)).toBeLessThan(EPSILON);
    expect(Math.abs(restored.left - before.left)).toBeLessThan(EPSILON);
  });
});
