import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const fixturesRoot = join(repositoryRoot, "fixtures", "sanitized");
const requiredFiles = ["metadata.json", "input.json", "normalized-events.json", "expected-postings.json", "expected-projection.json", "expected-errors.json"];
const errors = [];
let canonicalHashCount = 0;
let expenseResultCount = 0;

function fail(location, message) {
  errors.push(`${location}: ${message}`);
}

function compareUnicodeScalar(left, right) {
  const a = Array.from(left, (character) => character.codePointAt(0));
  const b = Array.from(right, (character) => character.codePointAt(0));
  for (let index = 0; index < Math.min(a.length, b.length); index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

function canonicalize(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) throw new Error(`non-safe canonical integer ${value}`);
    return String(value);
  }
  if (typeof value === "string") return JSON.stringify(value.normalize("NFC"));
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(",")}]`;
  if (typeof value === "object") {
    return `{${Object.keys(value).sort(compareUnicodeScalar).map((key) => `${JSON.stringify(key.normalize("NFC"))}:${canonicalize(value[key])}`).join(",")}}`;
  }
  throw new Error(`unsupported type ${typeof value}`);
}

const sha256 = (value) => `sha256:${createHash("sha256").update(canonicalize(value), "utf8").digest("hex")}`;

function decimalParts(value) {
  const match = /^(-?)(0|[1-9][0-9]*)(?:\.([0-9]+))?$/.exec(value);
  if (!match) return null;
  const fraction = match[3] ?? "";
  return {
    coefficient: BigInt(`${match[1]}${match[2]}${fraction}`),
    scale: fraction.length,
    significantDigits: `${match[2]}${fraction}`.replace(/^0+/, "").length || 1,
    negativeZero: match[1] === "-" && /^0*$/.test(`${match[2]}${fraction}`),
  };
}

function compareDecimal(left, right) {
  const a = decimalParts(left);
  const b = decimalParts(right);
  if (!a || !b) throw new Error(`invalid decimal comparison: ${left}, ${right}`);
  const scale = Math.max(a.scale, b.scale);
  const ac = a.coefficient * 10n ** BigInt(scale - a.scale);
  const bc = b.coefficient * 10n ** BigInt(scale - b.scale);
  return ac < bc ? -1 : ac > bc ? 1 : 0;
}

function sumDecimals(values) {
  const parsed = values.map(decimalParts);
  if (parsed.some((value) => value === null)) throw new Error("invalid decimal sum");
  const scale = Math.max(0, ...parsed.map((value) => value.scale));
  const total = parsed.reduce((sum, value) => sum + value.coefficient * 10n ** BigInt(scale - value.scale), 0n);
  const negative = total < 0n;
  const digits = (negative ? -total : total).toString().padStart(scale + 1, "0");
  if (scale === 0) return `${negative ? "-" : ""}${digits}`;
  return `${negative ? "-" : ""}${digits.slice(0, -scale)}.${digits.slice(-scale)}`;
}

function isFinancialKey(key) {
  if (/^(?:include|selected|first|last)_/.test(key)) return false;
  if (/(?:^|_)(?:count|days|version|sequence|row|rows|rank|watermark|scale|digits|items|id|ids|scope|date|currency|policy|result|bridge|category|categories)(?:_|$)/.test(key)) return false;
  return /(?:^|_)(?:amount|balance|cost|price|rate|pnl|value|quantity|proceeds|fee|tax|dividend|expense|income|refund|reimbursement|tolerance|difference|cash|subtotal|return|percentage|ratio)(?:_|$)/.test(key);
}

function scaleLimit(key) {
  if (/(?:^|_)(?:rate|final_rate|rate_to_base)(?:_|$)/.test(key)) return 15;
  if (/(?:^|_)unit_price(?:_|$)/.test(key) || key === "price") return 12;
  if (key !== "quantity_delta" && /(?:^|_)quantity(?:_|$)/.test(key)) return 12;
  if (/(?:base_value|cost|pnl|return|subtotal|difference|quantity_delta|cost_delta|average_cost|market_value)/.test(key)) return 18;
  return 8;
}

function validateScalarDomain(value, key, location, strictScale) {
  if (typeof value === "number" && (!Number.isSafeInteger(value) || value < 0)) {
    fail(location, `JSON number must be a non-negative safe integer, got ${value}`);
  }
  if (Array.isArray(value) || (value !== null && typeof value === "object")) return;
  if (location.includes(".decimal_contract.max_scale.")) return;
  if (typeof value === "string" && /^(?:expense|refund|market-data)-policy-v1$|^expense-bucket-policy-v1$/.test(value)) return;
  if (!isFinancialKey(key) || value === null) return;
  if (typeof value !== "string") {
    fail(location, `financial field ${key} must be a decimal string, got ${typeof value}`);
    return;
  }
  const parsed = decimalParts(value);
  if (!parsed) {
    fail(location, `financial field ${key} is not a canonical decimal string: ${value}`);
    return;
  }
  if (parsed.negativeZero) fail(location, `financial field ${key} uses forbidden negative zero`);
  if (parsed.significantDigits > 28) fail(location, `financial field ${key} exceeds 28 significant digits`);
  if (strictScale && parsed.scale > scaleLimit(key)) {
    fail(location, `financial field ${key} scale ${parsed.scale} exceeds ${scaleLimit(key)}`);
  }
}

function walk(value, location, visitor, key = "", strictScale = true) {
  visitor(value, key, location, strictScale);
  if (Array.isArray(value)) {
    value.forEach((item, index) => walk(item, `${location}[${index}]`, visitor, key, strictScale));
  } else if (value && typeof value === "object") {
    for (const [childKey, childValue] of Object.entries(value)) {
      const childStrict = strictScale && !(value.kind === "failure" || value.scenario_id === "failure" || value.scenarioId === "failure");
      walk(childValue, `${location}.${childKey}`, visitor, childKey, childStrict);
    }
  }
}

function validateHashes(value, location) {
  if (Array.isArray(value)) {
    value.forEach((item, index) => validateHashes(item, `${location}[${index}]`));
    return;
  }
  if (!value || typeof value !== "object") return;
  if (Object.hasOwn(value, "canonical_hash")) {
    canonicalHashCount += 1;
    const copy = structuredClone(value);
    const actual = copy.canonical_hash;
    delete copy.canonical_hash;
    let expected;
    try {
      expected = sha256(copy);
    } catch (error) {
      fail(location, `cannot canonicalize: ${error.message}`);
      expected = null;
    }
    if (expected !== null && actual !== expected) fail(location, `canonical hash mismatch; expected ${expected}, got ${actual}`);
  }
  for (const [key, child] of Object.entries(value)) validateHashes(child, `${location}.${key}`);
}

function validateDrilldownContext(context, location) {
  const stack = [[context, location]];
  while (stack.length > 0) {
    const [value, current] = stack.pop();
    if (Array.isArray(value)) {
      fail(current, "drilldown context must not contain arrays or event ID lists");
    } else if (value && typeof value === "object") {
      for (const [key, child] of Object.entries(value)) {
        if (key === "event_ids" || key === "event_id_list") fail(`${current}.${key}`, "unbounded event ID list is forbidden");
        stack.push([child, `${current}.${key}`]);
      }
    }
  }
}

function validateExpenseResult(result, location) {
  expenseResultCount += 1;
  const bucketSum = sumDecimals(result.buckets.map((bucket) => bucket.amount));
  if (compareDecimal(bucketSum, result.summary.valued_subtotal) !== 0) {
    fail(location, `bucket sum ${bucketSum} does not equal valued subtotal ${result.summary.valued_subtotal}`);
  }
  if (result.summary.total_expense !== null && compareDecimal(bucketSum, result.summary.total_expense) !== 0) {
    fail(location, `complete bucket sum ${bucketSum} does not equal total ${result.summary.total_expense}`);
  }
  if (result.summary.total_expense === null && result.summary.label !== "Valued expense subtotal") {
    fail(location, "missing-FX result must use the valued-subtotal label");
  }
  for (let index = 1; index < result.buckets.length; index += 1) {
    const previous = result.buckets[index - 1];
    const current = result.buckets[index];
    const amountOrder = compareDecimal(previous.amount, current.amount);
    if (amountOrder < 0 || (amountOrder === 0 && compareUnicodeScalar(previous.bucket_id, current.bucket_id) > 0)) {
      fail(`${location}.buckets[${index}]`, "buckets are not ordered by amount DESC, bucket_id ASC");
    }
  }
  const positive = result.buckets.filter((bucket) => compareDecimal(bucket.amount, "0") > 0);
  for (const bucket of result.buckets) {
    if (!Number.isSafeInteger(bucket.share_basis_points) || bucket.share_basis_points < 0) {
      fail(location, "bucket share_basis_points must be a non-negative safe integer");
    }
  }
  const expectedTopIds = positive.slice(0, 10).map((bucket) => bucket.bucket_id);
  const actualTopIds = result.top10.items.map((bucket) => bucket.bucket_id);
  if (JSON.stringify(actualTopIds) !== JSON.stringify(expectedTopIds)) fail(location, "Top 10 is not derived from canonical bucket order");
  result.top10.items.forEach((item, index) => {
    if (item.share_basis_points !== positive[index].share_basis_points) fail(location, "Top 10 share basis points differ from the full bucket row");
  });
  if (positive.length > 10) {
    if (result.top10.other === null) {
      fail(location, "Top 10 remainder is missing");
    } else {
      const expectedOther = sumDecimals(positive.slice(10).map((bucket) => bucket.amount));
      if (compareDecimal(expectedOther, result.top10.other.amount) !== 0) fail(location, "Top 10 remainder amount is inconsistent");
    }
  } else if (result.top10.other !== null) {
    fail(location, "Top 10 remainder must be null for ten or fewer positive buckets");
  }
  walk(result, location, (value, key, current) => {
    if (key === "drilldown_context") validateDrilldownContext(value, current);
    if (key === "event_ids" || key === "event_id_list") fail(current, "expense query result must not contain event ID arrays");
  });
}

function visitExpenseResults(value, location) {
  if (Array.isArray(value)) {
    value.forEach((item, index) => visitExpenseResults(item, `${location}[${index}]`));
    return;
  }
  if (!value || typeof value !== "object") return;
  if (value.contract === "expense-analysis-query-result/v1") validateExpenseResult(value, location);
  for (const [key, child] of Object.entries(value)) visitExpenseResults(child, `${location}.${key}`);
}

const fixtureDirectories = readdirSync(fixturesRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && /^[0-9]{2}-/.test(entry.name))
  .map((entry) => entry.name)
  .sort(compareUnicodeScalar);

if (fixtureDirectories.length !== 31) fail(fixturesRoot, `expected 31 numbered fixture groups, found ${fixtureDirectories.length}`);

const coveredRules = new Set();
for (let index = 0; index < fixtureDirectories.length; index += 1) {
  const fixtureName = fixtureDirectories[index];
  const expectedPrefix = String(index + 1).padStart(2, "0");
  if (!fixtureName.startsWith(`${expectedPrefix}-`)) fail(fixtureName, `expected case prefix ${expectedPrefix}`);
  const directory = join(fixturesRoot, fixtureName);
  const presentJson = readdirSync(directory).filter((name) => name.endsWith(".json")).sort(compareUnicodeScalar);
  if (JSON.stringify(presentJson) !== JSON.stringify([...requiredFiles].sort(compareUnicodeScalar))) {
    fail(fixtureName, `expected exactly ${requiredFiles.join(", ")}`);
  }
  const documents = new Map();
  for (const fileName of requiredFiles) {
    const path = join(directory, fileName);
    try {
      const text = readFileSync(path, "utf8");
      if (/(?:[A-Za-z]:\\Users\\|\/Users\/|\/home\/|多币种个人账本v1\.3\.0\.xlsx|-----BEGIN (?:RSA )?PRIVATE KEY-----|sk-[A-Za-z0-9]{20,}|[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,})/i.test(text)) {
        fail(`${fixtureName}/${fileName}`, "privacy scan found a private path, workbook name, credential or email pattern");
      }
      const document = JSON.parse(text);
      documents.set(fileName, document);
      walk(document, `${fixtureName}/${fileName}`, validateScalarDomain);
      validateHashes(document, `${fixtureName}/${fileName}`);
      visitExpenseResults(document, `${fixtureName}/${fileName}`);
    } catch (error) {
      fail(`${fixtureName}/${fileName}`, `parse or validation exception: ${error.message}`);
    }
  }
  const metadata = documents.get("metadata.json");
  if (!metadata) continue;
  if (metadata.fixture_id !== fixtureName) fail(`${fixtureName}/metadata.json`, "fixture_id must equal directory name");
  if (metadata.case_number !== index + 1) fail(`${fixtureName}/metadata.json`, "case_number does not match directory prefix");
  metadata.related_rules.forEach((rule) => coveredRules.add(rule));
  for (const kind of ["normal", "boundary", "failure"]) {
    const entry = metadata.coverage[kind];
    if (!entry || entry.scenario_ids.length === 0) fail(`${fixtureName}/metadata.json`, `${kind} coverage is empty`);
    const uncovered = metadata.related_rules.filter((rule) => !entry.rule_ids.includes(rule));
    if (uncovered.length > 0) fail(`${fixtureName}/metadata.json`, `${kind} coverage omits ${uncovered.join(", ")}`);
  }
  const scenarioSets = [];
  for (const fileName of requiredFiles.slice(1)) {
    const document = documents.get(fileName);
    if (!document) continue;
    if (document.fixture_id !== fixtureName) fail(`${fixtureName}/${fileName}`, "fixture_id mismatch");
    scenarioSets.push(document.scenarios.map((item) => item.scenario_id).sort(compareUnicodeScalar));
  }
  const expectedScenarios = ["boundary", "failure", "normal"];
  scenarioSets.forEach((set, setIndex) => {
    if (JSON.stringify(set) !== JSON.stringify(expectedScenarios)) fail(`${fixtureName}/${requiredFiles[setIndex + 1]}`, "must contain normal, boundary and failure scenarios exactly once");
  });
  const errorDocument = documents.get("expected-errors.json");
  if (errorDocument) {
    const failure = errorDocument.scenarios.find((item) => item.scenario_id === "failure");
    const normal = errorDocument.scenarios.find((item) => item.scenario_id === "normal");
    if (!failure || failure.errors.length === 0) fail(`${fixtureName}/expected-errors.json`, "failure scenario must have at least one stable error");
    if (normal && normal.errors.length > 0) fail(`${fixtureName}/expected-errors.json`, "normal scenario must not have errors");
  }
  const postings = documents.get("expected-postings.json");
  if (postings) {
    postings.scenarios.forEach((item) => {
      const expected = sha256(item.postings);
      if (item.sequence_hash !== expected) fail(`${fixtureName}/expected-postings.json`, `${item.scenario_id} sequence hash mismatch`);
    });
  }
}

const financialRulesPath = join(repositoryRoot, "docs", "financial-rules.md");
const requiredRules = existsSync(financialRulesPath)
  ? [...new Set([...readFileSync(financialRulesPath, "utf8").matchAll(/`([A-Z]+-[0-9]{3})`/g)].map((match) => match[1]))].sort(compareUnicodeScalar)
  : [];
const missingRules = requiredRules.filter((rule) => !coveredRules.has(rule));
if (missingRules.length > 0) fail("docs/financial-rules.md", `rules without normal/boundary/failure fixture coverage: ${missingRules.join(", ")}`);

const case30 = JSON.parse(readFileSync(join(fixturesRoot, "30-canonical-row-order", "expected-projection.json"), "utf8"));
const case30Normal = case30.scenarios.find((item) => item.scenario_id === "normal").state;
if (case30Normal.result_from_order_a.canonical_hash !== case30Normal.result_from_order_b.canonical_hash) {
  fail("30-canonical-row-order", "physical row order variants must have identical canonical hashes");
}

if (canonicalHashCount < 31 * 6) fail(fixturesRoot, `expected at least one canonical hash per fixture file, checked ${canonicalHashCount}`);
if (expenseResultCount < 20) fail(fixturesRoot, `expected broad expense query coverage, checked ${expenseResultCount} results`);

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else {
  console.log(`Validated 31 fixture groups, ${requiredRules.length} financial rules, ${canonicalHashCount} canonical hashes and ${expenseResultCount} expense query results.`);
}
