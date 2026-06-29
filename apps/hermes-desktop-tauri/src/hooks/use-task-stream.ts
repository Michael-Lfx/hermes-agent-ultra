import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { taskQueryKeys } from '@/hooks/use-task-queries'

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export function useTaskStream(taskId: string | null) {
  const queryClient = useQueryClient()

  useEffect(() => {
    if (!taskId || !isTauri()) return

    const streamId = `task:${taskId}`
    let unlisten: (() => void) | undefined

    void (async () => {
      const dispose = await listen<unknown>('terra:task-stream', () => {
        void queryClient.invalidateQueries({ queryKey: taskQueryKeys.events(taskId) })
        void queryClient.invalidateQueries({ queryKey: taskQueryKeys.detail(taskId) })
      })
      unlisten = dispose
      await invoke('subscribe_task_stream', { taskId, streamId })
    })()

    return () => {
      unlisten?.()
      void invoke('cancel_task_stream', { streamId }).catch(() => undefined)
    }
  }, [queryClient, taskId])
}
