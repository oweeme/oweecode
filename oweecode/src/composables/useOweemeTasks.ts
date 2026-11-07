// "Tasks" tab of the Oweeme panel: the board of the project linked to the open
// room (rooms carry projectUuid/projectKey), task detail, comments and moves.
// Moving a card here moves it in Oweeme, and the client's proposal shows the new progress.
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useOweemeChat } from './useOweemeChat'

export interface OweemeTask {
  uuid: string
  key: string
  title: string
  description: string | null
  status: string
  priority: 'low' | 'medium' | 'high' | 'urgent'
  estimateHours: number | null
  commentCount: number
  assignee: { name: string; slug: string } | null
}
export interface OweemeList { uuid: string; name: string; position: number; tasks: OweemeTask[] }
export interface OweemeComment { uuid: string; body: string; authorName: string; createdAt: string; isMe: boolean }

const boards = ref<Array<{ uuid: string; name: string }>>([])
const boardId = ref<string | null>(null)
const lists = ref<OweemeList[]>([])
const openTaskId = ref<string | null>(null)
const comments = ref<OweemeComment[]>([])
const loading = ref(false)

/** "bella-7-paypal" or "feature/BELLA-7" with key BELLA → "BELLA-7". */
export function taskKeyFromText(text: string | null | undefined, projectKey: string | null | undefined): string | null {
  if (!text || !projectKey) return null
  const m = new RegExp(`(?:^|[^a-z0-9])${projectKey}-(\\d+)(?![0-9])`, 'i').exec(text)
  return m ? `${projectKey.toUpperCase()}-${m[1]}` : null
}

export function useOweemeTasks() {
  const chat = useOweemeChat()
  const base = () => chat.baseUrl.value
  const project = computed(() => chat.activeRoom.value?.projectUuid ?? null)
  const projectKey = computed(() => chat.activeRoom.value?.projectKey ?? null)
  const allTasks = computed(() => lists.value.flatMap((l) => l.tasks.map((t) => ({ ...t, list: l }))))
  const openTask = computed(() => allTasks.value.find((t) => t.uuid === openTaskId.value) ?? null)

  async function guard<T>(fn: () => Promise<T>) {
    chat.error.value = ''
    try { return await fn() } catch (e) { chat.error.value = String(e); return undefined }
  }

  async function loadBoards() {
    boards.value = []; lists.value = []; boardId.value = null
    if (!project.value) return
    const list = await guard(() => invoke<Array<{ uuid: string; name: string }>>('oweeme_boards', { base: base(), project: project.value }))
    boards.value = list ?? []
    boardId.value = boards.value[0]?.uuid ?? null
    await loadKanban()
  }

  async function loadKanban() {
    if (!boardId.value) return
    loading.value = true
    const k = await guard(() => invoke<{ lists: OweemeList[] }>('oweeme_kanban', { base: base(), board: boardId.value }))
    loading.value = false
    if (k) lists.value = k.lists
  }

  async function selectBoard(id: string) { boardId.value = id; await loadKanban() }

  async function openTaskByUuid(uuid: string | null) {
    openTaskId.value = uuid
    comments.value = []
    if (uuid) comments.value = (await guard(() => invoke<OweemeComment[]>('oweeme_comments', { base: base(), task: uuid }))) ?? []
  }

  const findByKey = (key: string | null) => (key ? allTasks.value.find((t) => t.key.toUpperCase() === key.toUpperCase()) ?? null : null)

  async function move(taskUuid: string, listUuid: string) {
    if (await guard(() => invoke('oweeme_move_task', { base: base(), task: taskUuid, list: listUuid })) !== undefined) await loadKanban()
  }

  /** The column whose name means "review", or the one before the last. */
  function reviewList(): OweemeList | null {
    return lists.value.find((l) => /review|revis|qa|prueba/i.test(l.name)) ?? lists.value[lists.value.length - 2] ?? null
  }

  async function comment(taskUuid: string, body: string) {
    if (!body.trim()) return false
    const ok = await guard(() => invoke('oweeme_add_comment', { base: base(), task: taskUuid, body: body.trim() }))
    if (ok !== undefined && openTaskId.value === taskUuid) await openTaskByUuid(taskUuid)
    return ok !== undefined
  }

  async function createTask(listUuid: string, title: string) {
    if (!title.trim()) return
    if (await guard(() => invoke('oweeme_create_task', { base: base(), list: listUuid, title: title.trim() })) !== undefined) await loadKanban()
  }

  return { boards, boardId, lists, allTasks, openTask, comments, loading, project, projectKey, loadBoards, loadKanban, selectBoard, openTaskByUuid, findByKey, move, reviewList, comment, createTask }
}
