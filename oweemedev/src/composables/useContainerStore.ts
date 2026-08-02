import { ref } from 'vue'

export interface ContainerInfo {
  id: string
  image: string
  name: string
  status: string
  ports: string
  created: string
  running: boolean
}

// Module-scope singleton state: survives ContainerPanel unmount/remount when the
// sidebar is toggled closed/open, same pattern as useDatabaseStore/useRedisStore.
const containers = ref<ContainerInfo[]>([])
const runtime = ref<string | null>(null)
const runtimeError = ref('')
const loading = ref(false)

export function useContainerStore() {
  return { containers, runtime, runtimeError, loading }
}
