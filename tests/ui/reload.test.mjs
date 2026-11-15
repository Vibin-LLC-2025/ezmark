// Contributor-only tests of ui/reload.js (`npm test`): the rules that keep a
// reader in place across a live reload. These are the acceptance cases of
// plan section 9, run against the functions that decide them. No DOM needed.

import assert from 'node:assert/strict';
import test from 'node:test';

import { restorePlan, signature, unchangedEnds } from '../../ui/reload.js';

/** Blocks are single letters; a document is a string of them. */
const keys = (/** @type {string} */ document) => document.split('');

/**
 * Where a reader on block `anchor` of `before` lands in `after`, written as
 * `=3` (the same block, now at index 3, offset kept), `^0` (the top of the
 * block at index 0), or `ratio`.
 */
function lands(/** @type {string} */ before, /** @type {number} */ anchor, /** @type {string} */ after) {
  const plan = restorePlan(keys(before), keys(after), anchor);
  if (!plan) return 'ratio';
  return `${plan.exact ? '=' : '^'}${plan.index}`;
}

test('an edit above the reader: the anchor is matched and its offset kept', () => {
  assert.equal(lands('ABCDE', 3, 'AxBCDE'), '=4');
  assert.equal(lands('ABCDE', 3, 'ACDE'), '=2', 'a block above was deleted');
  assert.equal(lands('ABCDE', 3, 'ABCDEx'), '=3', 'an edit below changes nothing');
});

test('an edit inside the anchor block: the nearest unique block above, at its top', () => {
  assert.equal(lands('ABCDE', 3, 'ABCdE'), '^2');
});

test('the anchor block is deleted: the nearest unique block above', () => {
  assert.equal(lands('ABCDE', 3, 'ABCE'), '^2');
  assert.equal(lands('ABCDE', 3, 'ABE'), '^1', 'and the one above that, if that went too');
});

test('an identical block inserted above the anchor: never the inserted copy', () => {
  // Plan section 9, round 4 counterexample: A X X B, reading the first X.
  assert.equal(lands('AXXB', 1, 'AXXXB'), '^0');
  assert.equal(lands('AXXB', 2, 'AXXXB'), '^0', 'reading the second X');
});

test('a balanced insert and delete: equal counts are not correspondence', () => {
  // Plan section 9, round 5 counterexample: A X B X C, reading the first X.
  assert.equal(lands('AXBXC', 1, 'AXXBC'), '^0');
  // Reading the second X: B is unique in both and above it.
  assert.equal(lands('AXBXC', 3, 'AXXBC'), '^3');
});

test('a block that becomes duplicated, or stops being duplicated, is not matched', () => {
  assert.equal(lands('ABC', 1, 'ABBC'), '^0', 'one B before, two after');
  assert.equal(lands('ABBC', 1, 'ABC'), '^0', 'two before, one after');
  assert.equal(lands('ABBC', 3, 'ABC'), '=2', 'C is unique in both');
});

test('one of several identical blocks, no edit nearby: the nearest unique block above', () => {
  assert.equal(lands('AXXXB', 3, 'AXXXBc'), '^0');
});

test('nothing unique at or above the anchor: fall back to scroll ratio', () => {
  assert.equal(lands('XXA', 1, 'XXXA'), 'ratio');
  assert.equal(lands('ABC', 1, 'xyz'), 'ratio', 'everything changed');
  assert.equal(lands('ABC', 0, 'xBC'), 'ratio', 'the first block changed and nothing is above it');
  assert.equal(lands('', 0, 'ABC'), 'ratio', 'an empty document');
});

test('a block moved elsewhere is followed, as two snapshots cannot tell a move from a rewrite', () => {
  assert.equal(lands('ABCDE', 1, 'ACDEB'), '=4');
});

test('blocks without a key are never matched', () => {
  const before = ['a', '', 'c'];
  assert.deepEqual(restorePlan(before, ['a', '', 'c'], 1), { index: 0, exact: false });
  assert.deepEqual(restorePlan(['', ''], ['', ''], 1), null);
});

test('an anchor past the end is treated as the last block', () => {
  assert.deepEqual(restorePlan(keys('AB'), keys('AB'), 9), { index: 1, exact: false });
});

test('unchanged blocks at both ends are counted, and the two ends never overlap', () => {
  const ends = (/** @type {string} */ before, /** @type {string} */ after) => {
    const { head, tail } = unchangedEnds(keys(before), keys(after));
    return `${head}+${tail}`;
  };
  assert.equal(ends('ABCDE', 'ABxDE'), '2+2');
  assert.equal(ends('ABCDE', 'ABCDE'), '5+0', 'identical: all head');
  assert.equal(ends('ABCDE', 'ABCDEx'), '5+0', 'appended');
  assert.equal(ends('ABCDE', 'xABCDE'), '0+5', 'prepended');
  assert.equal(ends('AAA', 'AA'), '2+0', 'repeats are not counted twice');
  assert.equal(ends('AB', 'AxB'), '1+1');
  assert.equal(ends('ABAB', 'AB'), '2+0');
  assert.equal(ends('', 'AB'), '0+0');
  assert.equal(ends('AB', ''), '0+0');
  assert.equal(ends('ABC', 'xyz'), '0+0');
});

test('a signature tells HTML apart by content and by length', () => {
  assert.equal(signature('<p>a</p>'), signature('<p>a</p>'));
  assert.notEqual(signature('<p>a</p>'), signature('<p>b</p>'));
  assert.notEqual(signature('ab'), signature('ba'));
  assert.match(signature(''), /^0:\d+$/);
  assert.match(signature('<p>ünï ✓</p>'), /^12:\d+$/);
});
