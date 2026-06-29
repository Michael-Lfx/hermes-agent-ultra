export async function uploadImageArtifact(file: File, taskId?: string): Promise<string | null> {
  const form = new FormData()
  form.append('file', file)
  if (taskId) form.append('task_id', taskId)

  const res = await fetch('/api/artifacts/upload', {
    method: 'POST',
    body: form
  })

  if (!res.ok) return null
  const body = (await res.json()) as { artifact_id?: string }
  return body.artifact_id ?? null
}
