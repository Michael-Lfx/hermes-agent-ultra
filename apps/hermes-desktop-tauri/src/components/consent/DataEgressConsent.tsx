import { useT } from '@/i18n/useT'

interface DataEgressConsentProps {
  verticalName: string
  providers: string[]
  onAccept?: () => void
  onDecline?: () => void
}

export function DataEgressConsent({ verticalName, providers, onAccept, onDecline }: DataEgressConsentProps) {
  const t = useT('settings')

  return (
    <dialog open className="terra-consent-dialog">
      <h3>{t('title', 'Data egress consent')}</h3>
      <p>
        {verticalName} {t('body', 'may send data to')}: {providers.join(', ')}
      </p>
      <footer>
        <button type="button" onClick={onDecline}>
          {t('decline', 'Use local tier only')}
        </button>
        <button type="button" onClick={onAccept}>
          {t('accept', 'I agree')}
        </button>
      </footer>
    </dialog>
  )
}

export default DataEgressConsent
