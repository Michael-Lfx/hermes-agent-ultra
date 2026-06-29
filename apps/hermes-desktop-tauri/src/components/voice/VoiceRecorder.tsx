import { useCallback, useRef, useState } from 'react'

import { useT } from '@/i18n/useT'
import { uploadVoiceBlob } from '@/lib/voice-upload'

interface VoiceRecorderProps {
  onTranscript?: (text: string) => void
}

export function VoiceRecorder({ onTranscript }: VoiceRecorderProps) {
  const t = useT('voice')
  const [recording, setRecording] = useState(false)
  const [level, setLevel] = useState(0)
  const mediaRef = useRef<MediaRecorder | null>(null)
  const chunksRef = useRef<Blob[]>([])

  const start = useCallback(async () => {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
    const recorder = new MediaRecorder(stream)
    chunksRef.current = []
    recorder.ondataavailable = e => chunksRef.current.push(e.data)
    recorder.onstop = async () => {
      stream.getTracks().forEach(track => track.stop())
      const blob = new Blob(chunksRef.current, { type: 'audio/webm' })
      const text = await uploadVoiceBlob(blob)
      onTranscript?.(text)
    }
    recorder.start()
    mediaRef.current = recorder
    setRecording(true)
    setLevel(0.4)
  }, [onTranscript])

  const stop = useCallback((cancel = false) => {
    const recorder = mediaRef.current
    if (!recorder) return
    if (cancel) {
      recorder.onstop = null
    }
    recorder.stop()
    mediaRef.current = null
    setRecording(false)
    setLevel(0)
  }, [])

  return (
    <div className="terra-voice-recorder">
      <div className="terra-voice-recorder__meter" style={{ transform: `scaleX(${level})` }} />
      {recording ? (
        <>
          <button type="button" onClick={() => stop(false)}>
            {t('stop', 'Stop')}
          </button>
          <button type="button" onClick={() => stop(true)}>
            {t('cancel', 'Cancel')}
          </button>
        </>
      ) : (
        <button type="button" onClick={() => void start()}>
          {t('record', 'Record')}
        </button>
      )}
    </div>
  )
}

export default VoiceRecorder
