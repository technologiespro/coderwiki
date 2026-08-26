<script setup>
import { ref } from 'vue';
import { store } from '../store';

const props = defineProps({ node: Object, depth: { type: Number, default: 0 } });
const open = ref(props.depth < 2);
</script>

<template>
  <div>
    <button
      :data-testid="`tree-node-${node.path}`"
      class="w-full flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm text-left transition-colors duration-200"
      :class="store.selectedFile === node.path && node.type === 'file'
        ? 'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium'
        : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800'"
      :style="{ paddingLeft: `${8 + depth * 14}px` }"
      @click="node.type === 'dir' ? (open = !open) : (store.selectedFile = node.path)">
      <svg v-if="node.type === 'dir'" class="w-3.5 h-3.5 shrink-0 transition-transform duration-200" :class="open ? 'rotate-90' : ''"
        viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="9 18 15 12 9 6" />
      </svg>
      <svg v-if="node.type === 'dir'" class="w-4 h-4 shrink-0 text-slate-400 dark:text-slate-500"
        viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
      </svg>
      <svg v-else class="w-4 h-4 shrink-0 ml-[18px] text-slate-400 dark:text-slate-500"
        viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" />
      </svg>
      <span class="truncate">{{ node.name }}</span>
    </button>
    <div v-if="node.type === 'dir' && open">
      <TreeNode v-for="child in node.children" :key="child.path" :node="child" :depth="depth + 1" />
    </div>
  </div>
</template>
