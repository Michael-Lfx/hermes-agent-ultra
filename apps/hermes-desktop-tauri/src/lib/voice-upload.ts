export async function uploadVoiceBlob(blob: Blob, language = 'auto'): Promise<string> {
  const buffer = await blob.arrayBuffer()
  const bytes = new Uint8Array(buffer)
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }
  const audioBase64 = btoa(binary)

  const res = await fetch('/api/voice/transcribe', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ audio_base64: audioBase64, language })
  })

  if (!res.ok) {
    return ''
  }

  const body = (await res.json()) as { text?: string }
  return body.text ?? ''
}
