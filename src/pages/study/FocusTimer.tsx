import { useEffect, useState } from 'react';
import { IconPlay } from '../../components/icons';
import * as studyIpc from '../../lib/ipc/study';

export function FocusTimer({ planId, onLogged }: { planId: string; onLogged: () => void }) {
  const [running, setRunning] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => setSeconds((s) => s + 1), 1000);
    return () => window.clearInterval(t);
  }, [running]);

  const mmss = `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;

  const stop = async (log: boolean) => {
    setRunning(false);
    const minutes = Math.floor(seconds / 60);
    setSeconds(0);
    if (!log || minutes < 1) return;
    setBusy(true);
    try {
      await studyIpc.studyFocusLog(planId, null, minutes);
      onLogged();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <span style={{ display: 'inline-flex', gap: 8, alignItems: 'center' }}>
      {running && <span className="pg-chip on">专注中 {mmss}</span>}
      {!running ? (
        <button className="pg-chip" onClick={() => setRunning(true)}>
          <IconPlay size={12} />
          开始专注
        </button>
      ) : (
        <>
          <button className="pg-chip" disabled={busy} title="满 1 分钟才计入时长" onClick={() => void stop(true)}>
            结束并记录
          </button>
          <button className="pg-chip" disabled={busy} onClick={() => void stop(false)}>
            放弃
          </button>
        </>
      )}
    </span>
  );
}
