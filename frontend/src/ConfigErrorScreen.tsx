import type { EnvConfigIssue } from "./lib/envConfig";

/**
 * Rendered instead of the app when the build-time environment is missing or
 * malformed in production. Every unusable variable is listed by name so the
 * operator can fix the deployment instead of debugging individual contract
 * calls that fail for a configuration reason.
 */
export default function ConfigErrorScreen({ issues }: { issues: EnvConfigIssue[] }) {
  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="#top">
          <span className="brand__mark" aria-hidden="true">
            W
          </span>
          <span>
            <strong>Stellar Wrap</strong>
            <small>On-chain registry</small>
          </span>
        </a>
      </header>

      <main id="main-content" tabIndex={-1}>
        <section
          className="panel config-error"
          role="alert"
          aria-labelledby="config-error-heading"
        >
          <h1 id="config-error-heading">Configuration error</h1>
          <p>
            This deployment cannot start because its environment is incomplete
            or malformed. Fix the variables listed below and rebuild — no
            contract calls will be attempted until they are valid.
          </p>
          <ul className="config-error__list">
            {issues.map((issue) => (
              <li key={issue.variable}>
                <code>{issue.variable}</code> {issue.message}
              </li>
            ))}
          </ul>
          <p>
            See <code>frontend/.env.example</code> for the expected format of
            every variable.
          </p>
        </section>
      </main>
    </div>
  );
}
