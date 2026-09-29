import catalog from "../../../src/error_messages.json";

export type ContractErrorCopy = {
  variant: string;
  code: number;
  /** True when the account holder can fix the failure or wait it out. */
  actionable: boolean;
  message: string;
};

export const CONTRACT_ERROR_COPY: readonly ContractErrorCopy[] = catalog;

const BY_CODE = new Map<number, ContractErrorCopy>(
  CONTRACT_ERROR_COPY.map((entry) => [entry.code, entry]),
);

const CONTRACT_ERROR_CODE = /Error\(Contract,\s*#(\d+)\)/;

export function rawContractError(code: number): string {
  return `Error(Contract, #${code})`;
}

/**
 * User-facing text for one catalog entry. Actionable failures tell the
 * holder what to do. The rest keep the raw Soroban code for a bug report.
 */
export function formatContractError(entry: ContractErrorCopy): string {
  const raw = rawContractError(entry.code);
  if (entry.actionable) {
    return `${entry.message} (${raw})`;
  }
  return `${entry.message} You cannot resolve this from the app. Include ${raw} in a bug report.`;
}

export function renderContractError(code: number): string {
  const entry = BY_CODE.get(code);
  if (!entry) {
    const raw = rawContractError(code);
    return `The contract returned an unrecognized error. Include ${raw} in a bug report.`;
  }
  return formatContractError(entry);
}

/** Map a Soroban failure string to catalog text, or null when it has no contract code. */
export function renderContractFailure(failure: string): string | null {
  const match = CONTRACT_ERROR_CODE.exec(failure);
  if (!match) {
    return null;
  }
  return renderContractError(Number(match[1]));
}
