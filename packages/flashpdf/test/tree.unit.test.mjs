import assert from "node:assert/strict";

import { Component, forwardRef, memo } from "react";
import { Fragment, jsx, jsxs } from "react/jsx-runtime";
import { test } from "vitest";

import { reactTree } from "../dist/tree.js";

test("given React components and fragments, when normalizing, then returns host elements", async () => {
	const calls = [];
	const Nested = ({ label }) => {
		calls.push("nested");
		return jsx("p", { children: label });
	};
	const Row = ({ label }) => jsx("p", { children: label });
	const Parent = ({ label }) => {
		calls.push("parent");
		return jsx(Nested, { label });
	};
	const Sibling = ({ label }) => {
		calls.push("sibling");
		return jsx("p", { children: label });
	};
	const AsyncRow = async ({ label }) => jsx("p", { children: label });
	const tree = await reactTree(
		jsxs(Fragment, {
			children: [
				jsx(Parent, { label: "Invoice" }),
				jsx(Sibling, { label: "Due" }),
				jsx(Row, { label: "Subtotal" }),
				jsx(AsyncRow, { label: "Paid" }),
				false,
				null,
			],
		}),
	);

	assert.deepEqual(tree, [
		{ type: "p", props: { children: "Invoice" } },
		{ type: "p", props: { children: "Due" } },
		{ type: "p", props: { children: "Subtotal" } },
		{ type: "p", props: { children: "Paid" } },
		false,
		null,
	]);
	assert.deepEqual(calls, ["parent", "sibling", "nested"]);
});

test("given memo, forwardRef, and class components, when normalizing, then resolves each to its host element", async () => {
	const Memo = memo(({ label }) => jsx("p", { children: label }));
	const Ref = forwardRef(({ label }) => jsx("p", { children: label }));
	class Klass extends Component {
		render() {
			return jsx("p", { children: this.props.label });
		}
	}

	for (const Type of [Memo, Ref, Klass]) {
		assert.deepEqual(await reactTree(jsx(Type, { label: "x" })), {
			type: "p",
			props: { children: "x" },
		});
	}
	const reference = { current: null };
	const RefValue = forwardRef((props, ref) =>
		jsx("p", { children: `${props.label}:${ref === reference}:${"ref" in props}` }),
	);
	assert.deepEqual(await reactTree(jsx(RefValue, { label: "x", ref: reference })), {
		type: "p",
		props: { children: "x:true:false" },
	});
});

test("given iterable and bigint children, when normalizing, then matches React node semantics", async () => {
	assert.deepEqual(
		await reactTree(jsx("main", { children: new Set([1n, jsx("p", { children: "x" })]) })),
		{
			type: "main",
			props: { children: ["1", { type: "p", props: { children: "x" } }] },
		},
	);
});

test("given an intrinsic tree, when normalizing, then reuses the unchanged tree", async () => {
	const tree = {
		type: "main",
		props: { children: [{ type: "p", props: { children: "Invoice" } }] },
	};
	assert.equal(await reactTree(tree), tree);
});

test("given multi-child nesting, when normalizing, then 64 element levels fit and 65 do not", async () => {
	// Each level ends in a <span>, so `nest(n)` is n + 1 elements deep. The
	// children arrays React interposes must not count toward the budget.
	const nest = (depth) =>
		depth === 0
			? "x"
			: jsxs("div", { children: [nest(depth - 1), jsx("span", { children: "y" })] });

	await assert.doesNotReject(() => reactTree(nest(63)));
	await assert.rejects(() => reactTree(nest(64)), /nesting exceeds 64/);
});
