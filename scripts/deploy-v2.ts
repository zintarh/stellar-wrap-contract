import { createHash } from "crypto";
import { execFileSync } from "child_process";
import { promises as fs } from "fs";
import * as path from "path";

interface ContractDeployment {
  contract_id: string;
  wasm_hash: string;
  initialized: boolean;
  admin_address: string;
  admin_pubkey: string;
  roles: Record<string, string>;
}

interface DeploymentManifest {
  schema_version: 1;
  network: string;
  deployed_at: string;
  commit: string;
  deployer: string;
  contract_id: string;
  wasm_hash: string;
  admin_address: string;
  admin_pubkey: string;
  initialized: boolean;
  contracts: { wrap: ContractDeployment };
}

const root = path.resolve(__dirname, "..");
const manifestPath = path.resolve(root, process.env.MANIFEST_PATH || "deployments/manifest.json");

function runStellar(args: string[]): string {
  try {
    return execFileSync("stellar", args, { cwd: root, encoding: "utf8", stdio: ["inherit", "pipe", "inherit"] }).trim();
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`stellar ${args.join(" ")} failed: ${detail}`);
  }
}

function getManifestContractId(manifest: DeploymentManifest | null): string | undefined {
  return manifest?.contracts?.wrap?.contract_id || manifest?.contract_id;
}

async function loadManifest(): Promise<DeploymentManifest | null> {
  try {
    return JSON.parse(await fs.readFile(manifestPath, "utf8")) as DeploymentManifest;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
}

async function writeManifest(manifest: DeploymentManifest): Promise<void> {
  await fs.mkdir(path.dirname(manifestPath), { recursive: true });
  const temporaryPath = `${manifestPath}.tmp`;
  await fs.writeFile(temporaryPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  await fs.rename(temporaryPath, manifestPath);
}

function getCommit(): string {
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function contractIdFrom(output: string): string {
  const id = output.match(/C[A-Z2-7]{55}/)?.[0];
  if (!id) throw new Error(`Could not find a contract ID in Stellar CLI output: ${output}`);
  return id;
}

async function writeGitHubOutputs(contractId: string, adminAddress: string, action: string): Promise<void> {
  if (process.env.GITHUB_OUTPUT) {
    await fs.appendFile(process.env.GITHUB_OUTPUT, `contract_id=${contractId}\nadmin=${adminAddress}\naction=${action}\n`);
  }
}

async function main(): Promise<void> {
  const network = process.env.STELLAR_NETWORK || "testnet";
  const source = process.env.STELLAR_DEPLOYER_IDENTITY || "deployer";
  const adminSource = process.env.STELLAR_ADMIN_IDENTITY || source;
  const wasmPath = path.resolve(root, process.env.WASM_PATH || "target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm");
  const requestedContractId = process.env.INPUT_CONTRACT_ID || process.env.CONTRACT_ID;
  const requestedAdmin = process.env.INPUT_ADMIN_ADDRESS || process.env.ADMIN_ADDRESS;
  const adminPubkey = process.env.INPUT_ADMIN_PUBKEY || process.env.ADMIN_PUBKEY || process.env.STELLAR_ADMIN_PUBKEY || "";
  const initialize = (process.env.INPUT_INITIALIZE || process.env.INITIALIZE || "true").toLowerCase() !== "false";

  const wasm = await fs.readFile(wasmPath);
  const wasmHash = createHash("sha256").update(wasm).digest("hex");
  const deployer = runStellar(["keys", "address", source]);
  const existing = await loadManifest();
  const currentAdmin = existing?.contracts?.wrap?.admin_address || existing?.admin_address || deployer;
  const existingId = getManifestContractId(existing);
  const contractId = requestedContractId || existingId;
  const targetAdmin = requestedAdmin || currentAdmin;
  const isUpgrade = Boolean(contractId);
  const wasInitialized = existing?.contracts?.wrap?.initialized || existing?.initialized || false;
  const authoritySource = isUpgrade && wasInitialized && currentAdmin !== deployer
    ? adminSource
    : source;

  if (existing?.network && existing.network !== network) {
    throw new Error(`Manifest is for ${existing.network}, not ${network}; use a network-specific MANIFEST_PATH.`);
  }

  if (existing && existingId && !requestedContractId && existing.wasm_hash === wasmHash && existing.contracts?.wrap?.admin_address === targetAdmin) {
    console.log(`Deployment already matches the manifest on ${network}; no changes made.`);
    console.log(JSON.stringify(existing, null, 2));
    await writeGitHubOutputs(existingId, targetAdmin, "skipped");
    return;
  }

  if (!isUpgrade && initialize && !adminPubkey) {
    throw new Error("STELLAR_ADMIN_PUBKEY, ADMIN_PUBKEY, or INPUT_ADMIN_PUBKEY is required for initialization.");
  }
  if (!isUpgrade && initialize && !/^(?:0x)?[0-9a-fA-F]{64}$/.test(adminPubkey)) {
    throw new Error("The admin public key must be exactly 32 bytes encoded as 64 hexadecimal characters.");
  }

  const authority = runStellar(["keys", "address", authoritySource]);
  if (isUpgrade && wasInitialized && authority !== currentAdmin) {
    throw new Error(`Admin identity ${authoritySource} resolves to ${authority}, but the manifest records ${currentAdmin}.`);
  }

  let deployedId: string;
  let action: "deployed" | "upgraded";
  if (contractId) {
    runStellar(["contract", "install", "--wasm", wasmPath, "--source", source, "--network", network]);
    runStellar([
      "contract", "invoke", "--id", contractId, "--source", authoritySource, "--network", network,
      "--", "upgrade", "--wasm_hash", wasmHash,
    ]);
    deployedId = contractId;
    action = "upgraded";
  } else {
    const output = runStellar([
      "contract", "deploy", "--wasm", wasmPath, "--source", source, "--network", network,
      "--salt", wasmHash,
    ]);
    deployedId = contractIdFrom(output);
    action = "deployed";
    if (initialize) {
      runStellar([
        "contract", "invoke", "--id", deployedId, "--source", source, "--network", network,
        "--", "initialize", "--admin", deployer, "--admin_pubkey", adminPubkey.replace(/^0x/, ""),
      ]);
    }
  }

  let adminAddress = deployer;
  if ((initialize || wasInitialized) && targetAdmin !== currentAdmin) {
    runStellar([
      "contract", "invoke", "--id", deployedId, "--source", authoritySource, "--network", network,
      "--", "update_admin", "--new_admin", targetAdmin,
    ]);
    adminAddress = targetAdmin;
  } else if (isUpgrade && wasInitialized) {
    adminAddress = currentAdmin;
  } else if (!initialize) {
    adminAddress = "";
  }

  const initialized = initialize || Boolean(existing?.contracts?.wrap?.initialized);
  const deployment: ContractDeployment = {
    contract_id: deployedId,
    wasm_hash: wasmHash,
    initialized,
    admin_address: adminAddress,
    admin_pubkey: adminPubkey.replace(/^0x/, "") || existing?.contracts?.wrap?.admin_pubkey || "",
    roles: {
      admin: adminAddress,
      deployer: adminAddress ? (adminAddress === deployer ? "retained" : "renounced") : "unassigned",
    },
  };
  const manifest: DeploymentManifest = {
    schema_version: 1,
    network,
    deployed_at: new Date().toISOString(),
    commit: getCommit(),
    deployer,
    contract_id: deployedId,
    wasm_hash: wasmHash,
    admin_address: adminAddress,
    admin_pubkey: deployment.admin_pubkey,
    initialized: deployment.initialized,
    contracts: { wrap: deployment },
  };

  await writeManifest(manifest);
  console.log(`${action} contract ${deployedId} on ${network}`);
  console.log(`Manifest written to ${path.relative(root, manifestPath)}`);
  console.log(JSON.stringify(manifest, null, 2));

  await writeGitHubOutputs(deployedId, adminAddress, action);
}

main().catch((error) => {
  console.error("Deployment failed:", error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
