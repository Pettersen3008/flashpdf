import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	cpSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { cpus, release, tmpdir, totalmem } from "node:os";
import { dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const script = fileURLToPath(import.meta.url);
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const median = (values) => {
	const sorted = values.toSorted((a, b) => a - b);
	const middle = Math.floor(sorted.length / 2);
	return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

if (process.argv[2] === "--child") {
	await child(JSON.parse(process.argv[3]));
} else {
	main();
}

async function child({
	root,
	fonts,
	backend,
	tagged,
	warmups,
	runs,
	check,
	pdfPath,
	artifactSha256,
}) {
	const artifact = join(
		root,
		backend === "native" ? "flashpdf.node" : "wasm/flashpdf_wasm_bg.wasm",
	);
	assert.equal(sha256(readFileSync(artifact)), artifactSha256, "snapshot artifact changed");
	const start = check ? 0 : performance.now();
	const { createRenderer } = await import(pathToFileURL(join(root, "dist/render.js")));
	let loadBackend;
	if (backend === "native") {
		const addon = createRequire(import.meta.url)(artifact);
		assert.equal(typeof addon.PdfRenderer, "function", "native addon did not load");
		const { nativeBackend } = await import(pathToFileURL(join(root, "dist/native.js")));
		const native = nativeBackend(addon);
		loadBackend = async () => native;
	} else {
		({ loadBackend } = await import(pathToFileURL(join(root, "dist/wasm.js"))));
	}
	const { invoice } = await import("./invoice.mjs");
	const fixture = await invoice(root, fonts, tagged);
	const render = createRenderer(loadBackend);
	const firstPdf = await render(fixture.create(), fixture.options);
	const coldMilliseconds = check ? undefined : performance.now() - start;
	const first = Buffer.from(firstPdf);
	assert.equal(first.subarray(0, 5).toString(), "%PDF-", "invalid PDF header");
	const warmMilliseconds = [];
	for (let i = 0; i < (check ? 1 : warmups + runs); i++) {
		const tick = check ? 0 : performance.now();
		const pdf = await render(fixture.create(), fixture.options);
		if (!check && i >= warmups) warmMilliseconds.push(performance.now() - tick);
		assert(first.equals(Buffer.from(pdf)), "repeated renders changed PDF bytes");
	}
	writeFileSync(pdfPath, first);
	console.log(
		JSON.stringify({
			backend,
			tagged,
			artifactSha256,
			coldMilliseconds,
			warmMilliseconds,
			warmMedianMilliseconds: check ? undefined : median(warmMilliseconds),
			peakRssBytes: process.resourceUsage().maxRSS * 1024,
			pdfBytes: first.length,
			pdfSha256: sha256(first),
		}),
	);
}

function main() {
	const { values: args } = parseArgs({
		options: {
			baseline: { type: "string" },
			candidate: { type: "string" },
			backend: { type: "string" },
			fonts: { type: "string" },
			"baseline-sha256": { type: "string" },
			"candidate-sha256": { type: "string" },
			pairs: { type: "string", default: "10" },
			warmups: { type: "string", default: "10" },
			runs: { type: "string", default: "100" },
			check: { type: "boolean", default: false },
			help: { type: "boolean" },
		},
	});
	if (args.help) {
		console.log(
			"node packages/flashpdf/bench/compare.mjs --baseline <built-repo> --candidate <built-repo> --backend native|wasm --fonts <directory-with-inter-400.ttf-and-inter-700.ttf> --baseline-sha256 <selected-artifact-hash> --candidate-sha256 <selected-artifact-hash> [--pairs 10 --warmups 10 --runs 100] [--check]",
		);
		return;
	}
	for (const name of ["baseline", "candidate", "fonts", "baseline-sha256", "candidate-sha256"])
		assert(args[name], `--${name} is required`);
	assert(["native", "wasm"].includes(args.backend), "--backend must be native or wasm");
	const counts = Object.fromEntries(
		["pairs", "warmups", "runs"].map((name) => {
			const value = Number(args[name]);
			assert(
				Number.isSafeInteger(value) && value >= (name === "warmups" ? 0 : 1),
				`invalid --${name}`,
			);
			return [name, value];
		}),
	);
	const work = mkdtempSync(join(tmpdir(), "flashpdf-bench-"));
	const fonts = join(work, "fonts");
	mkdirSync(fonts);
	const fontHashes = {};
	for (const name of ["inter-400.ttf", "inter-700.ttf"]) {
		cpSync(join(resolve(args.fonts), name), join(fonts, name));
		fontHashes[name] = sha256(readFileSync(join(fonts, name)));
	}
	const revisions = {};
	try {
		for (const name of ["baseline", "candidate"])
			revisions[name] = snapshot(
				resolve(args[name]),
				join(work, name),
				args.backend,
				args[`${name}-sha256`],
			);
		const proofs = [];
		const samples = [];
		for (const tagged of [false, true]) {
			for (const revision of ["baseline", "candidate"]) {
				const outputs = ["native", "wasm"].map((backend) =>
					isolated({
						root: join(work, revision),
						fonts,
						backend,
						tagged,
						check: true,
						...counts,
						artifactSha256: revisions[revision].artifacts[backend].sha256,
						pdfPath: join(work, `${revision}-${backend}-${tagged}.pdf`),
					}),
				);
				assert.equal(
					outputs[0].pdfSha256,
					outputs[1].pdfSha256,
					`${revision}: native/WASM bytes differ`,
				);
				proofs.push({
					revision,
					tagged,
					pdfSha256: outputs[0].pdfSha256,
					pdfBytes: outputs[0].pdfBytes,
				});
			}
			if (args.check) continue;
			for (let pair = 0; pair < counts.pairs; pair++) {
				const order = pair % 2 ? ["candidate", "baseline"] : ["baseline", "candidate"];
				for (const revision of order) {
					const sample = isolated({
						root: join(work, revision),
						fonts,
						backend: args.backend,
						tagged,
						...counts,
						artifactSha256: revisions[revision].artifacts[args.backend].sha256,
						pdfPath: join(work, `${revision}-${args.backend}-${tagged}.pdf`),
					});
					assert.equal(
						sample.pdfSha256,
						proofs.find((proof) => proof.revision === revision && proof.tagged === tagged)
							.pdfSha256,
						"PDF bytes changed across processes",
					);
					samples.push({ revision, pair, ...sample });
				}
			}
		}
		const summary = [false, true].map((tagged) => {
			const matching = proofs.filter((proof) => proof.tagged === tagged);
			const byteEqual = matching[0].pdfSha256 === matching[1].pdfSha256;
			const group = (revision) =>
				samples.filter((sample) => sample.tagged === tagged && sample.revision === revision);
			const stats = (revision) =>
				Object.fromEntries(
					["coldMilliseconds", "warmMedianMilliseconds", "peakRssBytes", "pdfBytes"].map((key) => [
						key,
						median(group(revision).map((sample) => sample[key])),
					]),
				);
			const ratios = (key) =>
				group("baseline").map((sample, i) => group("candidate")[i][key] / sample[key]);
			return {
				tagged,
				byteEqual,
				...(!args.check && {
					baseline: stats("baseline"),
					candidate: stats("candidate"),
					pairedCandidateOverBaseline: Object.fromEntries(
						["coldMilliseconds", "warmMedianMilliseconds"].map((key) => {
							const values = ratios(key);
							return [
								key,
								{
									median: median(values),
									min: Math.min(...values),
									max: Math.max(...values),
									samples: values,
								},
							];
						}),
					),
				}),
			};
		});
		console.log(
			JSON.stringify(
				{
					measuredAt: new Date().toISOString(),
					checkOnly: args.check,
					backend: args.backend,
					environment: {
						node: process.version,
						v8: process.versions.v8,
						zlib: process.versions.zlib,
						gzip: version("gzip"),
						platform: process.platform,
						arch: process.arch,
						os: release(),
						cpu: cpus()[0]?.model,
						logicalCpus: cpus().length,
						totalMemoryBytes: totalmem(),
						rustc: version("rustc"),
						wasmPack: version("wasm-pack"),
					},
					counts,
					revisions,
					fontHashes,
					harnessSha256: sha256(
						Buffer.concat([
							readFileSync(script),
							readFileSync(join(dirname(script), "invoice.mjs")),
						]),
					),
					method:
						"Cold includes imports, font reads, backend initialization, tree creation and first render in a fresh process; excludes process launch. Warm includes tree creation and render after discarded warmups; excludes byte checks. Each pair alternates order. Ratios below 1 favor candidate. Range describes observed process pairs, not a confidence interval; small or overlapping differences need more quiet runs. Byte changes block claims until separately reviewed for semantics. Byte equality does not establish a quiet host; review background CPU activity separately. No forced GC; RSS is process high-water mark including verification.",
					proofs,
					summary,
					samples,
					pdfDirectory: work,
				},
				null,
				2,
			),
		);
	} finally {
		for (const name of ["baseline", "candidate", "fonts"])
			rmSync(join(work, name), { recursive: true, force: true });
	}
}

function isolated(options) {
	const result = spawnSync(process.execPath, [script, "--child", JSON.stringify(options)], {
		encoding: "utf8",
		timeout: 120_000,
		maxBuffer: 16 * 1024 * 1024,
	});
	assert.equal(
		result.status,
		0,
		`${options.backend} child failed: ${result.error ?? result.stderr}`,
	);
	return JSON.parse(result.stdout);
}

function snapshot(repo, destination, backend, expectedHash) {
	assert.match(expectedHash, /^[a-f0-9]{64}$/, "artifact SHA-256 must be 64 lowercase hex digits");
	const pkg = join(repo, "packages/flashpdf");
	const nativePlatform = `${process.platform}-${process.arch}${process.platform === "linux" ? "-gnu" : ""}`;
	mkdirSync(destination);
	for (const dir of ["dist", "wasm"])
		cpSync(join(pkg, dir), join(destination, dir), { recursive: true });
	writeFileSync(join(destination, "package.json"), '{"type":"module"}');
	cpSync(join(pkg, "npm", nativePlatform, "flashpdf.node"), join(destination, "flashpdf.node"));
	assert.equal(
		sha256(
			readFileSync(
				join(destination, backend === "native" ? "flashpdf.node" : "wasm/flashpdf_wasm_bg.wasm"),
			),
		),
		expectedHash,
		`${backend} artifact SHA-256 mismatch`,
	);
	const artifacts = Object.fromEntries(
		[
			["native", "flashpdf.node"],
			["wasm", "wasm/flashpdf_wasm_bg.wasm"],
		].map(([name, path]) => {
			const bytes = readFileSync(join(destination, path));
			return [
				name,
				{
					sha256: sha256(bytes),
					rawBytes: bytes.length,
					releaseGzip9Bytes: execFileSync("gzip", ["-9", "-n", "-c"], {
						input: bytes,
						maxBuffer: bytes.length * 2 + 1024,
					}).length,
					nodeGzip9Bytes: gzipSync(bytes, { level: 9 }).length,
					brotli11Bytes: brotliCompressSync(bytes, {
						params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
					}).length,
				},
			];
		}),
	);
	const hashes = (dir) =>
		Object.fromEntries(
			readdirSync(join(destination, dir), { recursive: true, withFileTypes: true })
				.filter((entry) => entry.isFile())
				.map((entry) => {
					const path = join(entry.parentPath, entry.name);
					return [path.slice(destination.length + 1), sha256(readFileSync(path))];
				}),
		);
	return {
		commit: execFileSync("git", ["rev-parse", "HEAD"], { cwd: repo, encoding: "utf8" }).trim(),
		sourceStatus: execFileSync("git", ["status", "--short"], {
			cwd: repo,
			encoding: "utf8",
		}).trim(),
		packageVersion: JSON.parse(readFileSync(join(pkg, "package.json"))).version,
		buildProfile: readFileSync(join(repo, "Cargo.toml"), "utf8"),
		buildScriptsSha256: Object.fromEntries(
			["build.sh", "build-native.sh", "rust-toolchain.toml", "Cargo.lock", "pnpm-lock.yaml"].map(
				(name) => [name, sha256(readFileSync(join(repo, name)))],
			),
		),
		artifacts,
		javascript: hashes("dist"),
		wasmGlue: hashes("wasm"),
	};
}

function version(command) {
	const result = spawnSync(command, ["--version"], { encoding: "utf8" });
	return result.status === 0 ? (result.stdout || result.stderr).trim() : null;
}
