<script lang="ts" setup>
import { ref, watch } from "vue";

interface Props {
  enabled?: boolean;
  densityDissipation?: number;
  velocityDissipation?: number;
  pressure?: number;
  curl?: number;
  splatRadius?: number;
  splatForce?: number;
  transparent?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  enabled: false,
  densityDissipation: 3.5,
  velocityDissipation: 2,
  pressure: 0.1,
  curl: 3,
  splatRadius: 0.2,
  splatForce: 6000,
  transparent: true,
});

const enabled = ref(props.enabled);

watch(
  () => props.enabled,
  (value) => {
    enabled.value = value;
  },
);
</script>

<template>
  <div class="relative flex h-60 w-full flex-col items-center justify-center gap-4">
    <div class="flex flex-row items-center justify-center gap-4">
      <span>Enable Effect</span>
      <USwitch v-model="enabled" />
    </div>
    <span
      v-if="enabled"
      class="text-4xl font-semibold opacity-50"
    >
      Hover anywhere
    </span>
    <FluidCursor
      v-if="enabled"
      :density-dissipation="props.densityDissipation"
      :velocity-dissipation="props.velocityDissipation"
      :pressure="props.pressure"
      :curl="props.curl"
      :splat-radius="props.splatRadius"
      :splat-force="props.splatForce"
      :transparent="props.transparent"
    />
  </div>
</template>
