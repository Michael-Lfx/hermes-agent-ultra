import { useT } from '@/i18n/useT'

interface WelcomeProps {
  onNext?: () => void
}

export function Welcome({ onNext }: WelcomeProps) {
  const t = useT('onboarding')
  return (
    <section className="terra-onboarding-welcome">
      <h2>{t('welcome.title', 'Welcome to Terra')}</h2>
      <p>{t('welcome.body', 'Task-centric AI for research and automation.')}</p>
      <button type="button" onClick={onNext}>
        {t('welcome.next', 'Continue')}
      </button>
    </section>
  )
}

export function SignIn({ onNext }: WelcomeProps) {
  const t = useT('onboarding')
  return (
    <section className="terra-onboarding-signin">
      <h2>{t('signIn.title', 'Sign in')}</h2>
      <p>{t('signIn.body', 'Use email, WeChat, or OAuth providers.')}</p>
      <button type="button" onClick={onNext}>
        {t('signIn.skip', 'Continue as guest')}
      </button>
    </section>
  )
}

export function ProviderConfig({ onNext }: WelcomeProps) {
  const t = useT('onboarding')
  return (
    <section className="terra-onboarding-provider">
      <h2>{t('provider.title', 'Choose provider tier')}</h2>
      <p>{t('provider.body', 'Smart, Economic, or Local — change anytime in Settings.')}</p>
      <button type="button" onClick={onNext}>
        {t('provider.next', 'Next')}
      </button>
    </section>
  )
}

export function FirstTask({ onDone }: { onDone?: () => void }) {
  const t = useT('onboarding')
  return (
    <section className="terra-onboarding-first-task">
      <h2>{t('firstTask.title', 'Create your first task')}</h2>
      <p>{t('firstTask.body', 'Pick a vertical from the home screen to begin.')}</p>
      <button type="button" onClick={onDone}>
        {t('firstTask.done', 'Go to home')}
      </button>
    </section>
  )
}
