import { useEffect } from 'react';
import { CATALOG } from './errors';
import type { ErrorCode } from './errors';

type Props = { code: ErrorCode; onAction: (action: string) => void; detail?: string };

const ICONS: Record<string, string> = { info: 'ℹ', warning: '⚠', error: '✖', blocking: '⛔' };
const ACTION_LABELS: Record<string, string> = { retry: 'Try again', 'skip-track': 'Skip track', dismiss: 'Dismiss' };
const EXCLUDED_ACTIONS = new Set(['enter-token', 'change-server']);

function BannerContent({ code, onAction, detail }: Props) {
  const entry = CATALOG[code];
  const label = entry.severity.charAt(0).toUpperCase() + entry.severity.slice(1);
  return (
    <>
      <span aria-hidden="true">{ICONS[entry.severity]}</span>
      <span>{label}</span>
      <strong>{entry.title}</strong>
      <p>{entry.message}</p>
      {detail ? <p>{detail}</p> : null}
      {entry.actions.filter((action) => !EXCLUDED_ACTIONS.has(action)).map((action) => (
        <button key={action} type="button" onClick={() => onAction(action)}>{ACTION_LABELS[action] ?? action}</button>
      ))}
    </>
  );
}

export default function ErrorBanner(props: Props) {
  const entry = CATALOG[props.code];
  if (entry.silent) return null;
  return (
    <section className={`errbanner sev-${entry.severity}`} role={entry.announce === 'assertive' ? 'alert' : 'status'}>
      <BannerContent {...props} />
    </section>
  );
}

export function Toast(props: Props) {
  const entry = CATALOG[props.code];
  useEffect(() => {
    if (entry.silent) return;
    const id = setTimeout(() => props.onAction('dismiss'), 6000);
    return () => clearTimeout(id);
  }, [entry.silent, props]);
  if (entry.silent) return null;
  return (
    <div className="toast" role={entry.announce === 'assertive' ? 'alert' : 'status'}>
      <BannerContent {...props} />
    </div>
  );
}
