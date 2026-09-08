<script lang="ts" setup>
import { ref } from "vue";

interface Props {
  margin?: number;
  blurStdDeviation?: number;
}

const props = withDefaults(defineProps<Props>(), {
  margin: 20,
  blurStdDeviation: 6,
});

const tabs = ["Home", "Apps", "About", "My"];
const activeTab = ref(tabs[0]);
</script>

<template>
  <MorphingTabs
    :tabs="tabs"
    :active-tab="activeTab"
    :margin="props.margin"
    :blur-std-deviation="props.blurStdDeviation"
    @update:active-tab="activeTab = $event"
  />
</template>
