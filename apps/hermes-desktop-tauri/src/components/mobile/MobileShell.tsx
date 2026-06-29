import type { ReactNode } from 'react'

import { BottomTabBar } from '@/components/mobile/BottomTabBar'
import { useT } from '@/i18n/useT'

type MobileTab = 'home' | 'voice' | 'settings'

interface MobileShellProps {
  activeTab?: MobileTab
  onTabChange?: (tab: MobileTab) => void
  children?: ReactNode
}

export function MobileShell({ activeTab = 'home', onTabChange, children }: MobileShellProps) {
  const t = useT('app')

  return (
    <div className="terra-mobile-shell">
      <header className="terra-mobile-shell__header">
        <h1>{t('mobile.title', 'Terra')}</h1>
      </header>
      <main className="terra-mobile-shell__main">{children}</main>
      <BottomTabBar active={activeTab} onChange={onTabChange} />
    </div>
  )
}

export default MobileShell
