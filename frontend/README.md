# Stellar Wrap — Frontend

The frontend is a Vite + React + TypeScript single-page app that talks to the
`stellar-wrap` Soroban contract on Stellar testnet. This README walks you from a
fresh clone to a running app connected to a deployed contract.

## Prerequisites

- **Node.js >= 20.19.0** (see `engines` in `package.json`).
- **npm** (ships with Node).
- **Freighter** browser extension — the app is unusable without it (see below).

## 1. Install Freighter and switch it to testnet

The app signs transactions through the [Freighter](https://www.freighter.app/)
wallet extension. Without it, the app cannot connect or submit anything.

1. Install Freighter for your browser from <https://www.freighter.app/>.
2. Create or import an account.
3. Open Freighter → **Settings → Network** and select **Testnet**.
4. Fund the account on testnet (e.g. via the
   [Stellar Laboratory friendbot](https://laboratory.stellar.org/#account-creator?network=testnet))
   so it can pay fees.

Keep Freighter on **Testnet** — the app is configured for testnet contract ids.

## 2. Configure environment variables

Copy the example file and fill in the values:

```bash
cp .env.example .env
```

| Variable | Required | Meaning | Where to get it |
| --- | --- | --- | --- |
| `VITE_CONTRACT_ID` | yes | The Soroban contract id (starts with `C…`) the app reads from and writes to. | Output of a testnet deployment — see [Deploying your own contract](#4-deploying-your-own-contract). |
| `VITE_RPC_URL` | no | Soroban RPC endpoint used to simulate/submit transactions. Defaults to the public testnet RPC. | `https://soroban-testnet.stellar.org` for testnet. |
| `VITE_NETWORK_PASSPHRASE` | no | Network passphrase the SDK signs against. Defaults to testnet. | `Test SDF Network ; September 2015` for testnet. |

Only variables prefixed with `VITE_` are exposed to the browser. Restart the dev
server after changing `.env`.

## 3. Run, test, and build

```bash
npm install        # install dependencies
npm run dev        # start the local dev server (Vite)
npm run test       # run the unit tests (Vitest)
npm run build      # type-check and produce a production build in dist/
```

Other useful scripts: `npm run lint`, `npm run typecheck`, `npm run a11y`.

With Freighter on testnet and `VITE_CONTRACT_ID` set, open the dev server URL
and the app will connect to your deployed contract.

## 4. Deploying your own contract

To point the app at your own contract, deploy it to testnet first. The
repository ships a workflow for this:

- [`.github/workflows/deploy-testnet.yml`](../.github/workflows/deploy-testnet.yml)
  builds and deploys the contract to testnet and prints the resulting contract id.

Copy that contract id into `VITE_CONTRACT_ID` in your `.env`, restart
`npm run dev`, and the frontend will talk to your deployment.
