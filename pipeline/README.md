# Indexer and TTL renewal bot

The indexer stores active wrap keys in its SQLite database. The TTL renewal
command uses those indexed keys, asks Soroban RPC for each key's
`liveUntilLedgerSeq`, groups records by user, and submits bounded
`extend_ttl_batch` calls for records approaching expiry.

## Configuration

The command reads `pipeline/.env` (or the process environment):

| Variable | Required | Description |
| --- | --- | --- |
| `CONTRACT_ID` | yes | Registry contract to renew |
| `RPC_URL` | no | Soroban RPC URL; defaults to Stellar testnet |
| `DB_PATH` | no | Indexer SQLite database; defaults to `pipeline/indexer.db` |
| `STELLAR_NETWORK_PASSPHRASE` | yes | Exact passphrase for the RPC network |
| `RENEWER_SOURCE_ACCOUNT` | yes | Dedicated, funded low-privilege fee account public key |
| `RENEWER_THRESHOLD_DAYS` | no | Renew records at or below this horizon; defaults to 30 days |
| `RENEWER_MAX_SPEND_STROOPS` | yes | Maximum cumulative transaction fee authorized per run, in stroops |
| `RENEWER_SECRET_KEY` | execution only | Secret for the dedicated source account; keep it in a secret manager |

The bot is dry-run by default. Dry runs still query TTLs and simulate every
planned batch, check the configured per-run fee cap, and log planned records
and estimated fees. To submit transactions, explicitly run `npm run renew-ttl
-- --execute`. The keypair's public key must match
`RENEWER_SOURCE_ACCOUNT`. The total simulated transaction fee is checked
against the cap before each submission; the account should hold only the
amount needed for its operational fee budget.

Soroban renewal is permissionless, so user signatures are not requested. The
bot does not restore records that are already archived or absent from RPC;
those are counted separately and need an explicit restore workflow. The
day-to-ledger calculation uses the network's nominal 5-second ledger cadence,
so schedule the job well before the configured expiration boundary.

## Example schedule

Keep the indexer running to maintain the database, then invoke the renewer
daily from cron. Install pipeline dependencies once with `npm ci` (or
`npm install` if the lockfile needs refreshing), and provide the environment
values through a protected deployment environment:

```cron
0 9 * * * cd /opt/stellar-wrap-contract/pipeline && npm run renew-ttl -- --execute
```

Run it without `--execute` first and review `due`, `records_renewed`,
`estimated_fee`, `actual_spent_confirmed`, `unconfirmed`, and `cap` in the
command output. An unconfirmed submission is reported with its maximum
possible fee because its final charge is not yet known.
