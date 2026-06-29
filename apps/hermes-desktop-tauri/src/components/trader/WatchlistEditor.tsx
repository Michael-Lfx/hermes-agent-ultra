import { useCallback, useEffect, useState } from 'react'

import { useT } from '@/i18n/useT'

const STORAGE_KEY = 'terra.watchlist.symbols.v1'

function readSymbols(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    return Array.isArray(parsed) ? parsed.map(String) : []
  } catch {
    return []
  }
}

function writeSymbols(symbols: string[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(symbols))
}

export interface WatchlistRule {
  id: string
  symbol: string
  kind: 'pct_change' | 'volume' | 'announcement' | 'earnings'
  threshold: string
}

export function WatchlistEditor() {
  const t = useT('vertical')
  const [symbols, setSymbols] = useState<string[]>(() => readSymbols())
  const [draft, setDraft] = useState('')

  const persist = useCallback((next: string[]) => {
    setSymbols(next)
    writeSymbols(next)
    void fetch('/api/schedules', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        title: `Watchlist ${next.join(',')}`,
        prompt_template: `Monitor symbols: ${next.join(', ')}`,
        expr: '0 9 * * 1-5',
        timezone: 'Asia/Shanghai',
        vertical: 'trader'
      })
    }).catch(() => undefined)
  }, [])

  useEffect(() => {
    if (symbols.length > 0) return
    const loaded = readSymbols()
    if (loaded.length > 0) setSymbols(loaded)
  }, [symbols.length])

  const addSymbol = () => {
    const symbol = draft.trim().toUpperCase()
    if (!symbol || symbols.includes(symbol)) return
    persist([...symbols, symbol])
    setDraft('')
  }

  return (
    <section className="terra-watchlist-editor">
      <h3>{t('watchlist.title', 'Watchlist')}</h3>
      <div className="terra-watchlist-editor__add">
        <input
          value={draft}
          placeholder={t('watchlist.symbol', 'Symbol e.g. 600519')}
          onChange={e => setDraft(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && addSymbol()}
        />
        <button type="button" onClick={addSymbol}>
          {t('watchlist.add', 'Add')}
        </button>
      </div>
      <ul>
        {symbols.map(symbol => (
          <li key={symbol}>{symbol}</li>
        ))}
      </ul>
      <p className="terra-watchlist-editor__hint">
        {t('watchlist.rulesHint', 'Symbols persist locally and sync a trader cron schedule when backend is up.')}
      </p>
    </section>
  )
}

export default WatchlistEditor
