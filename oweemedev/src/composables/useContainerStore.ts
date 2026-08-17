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

export interface PodContainerInfo {
  id: string
  name: string
  status: string
}

export interface PodInfo {
  id: string
  name: string
  status: string
  created: string
  containers: PodContainerInfo[]
}

// Module-scope singleton state: survives ContainerPanel unmount/remount when the
// sidebar is toggled closed/open, same pattern as useDatabaseStore/useRedisStore.
const containers = ref<ContainerInfo[]>([])
const pods = ref<PodInfo[]>([])
const runtime = ref<string | null>(null)
const runtimeError = ref('')
const loading = ref(false)

export function useContainerStore() {
  return { containers, pods, runtime, runtimeError, loading }
}
