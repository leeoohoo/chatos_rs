<script setup lang="ts">
interface Props {
  color?: string;
  duration?: number;
  delay?: number;
}

const props = withDefaults(defineProps<Props>(), {
  color: "#e1251b",
  duration: 0.8,
  delay: 0.25,
});
</script>

<template>
  <div class="size-full max-w-lg items-center justify-center overflow-hidden p-8">
    <BoxReveal
      :color="props.color"
      :duration="props.duration"
      :delay="props.delay"
    >
      <p class="text-[3.5rem] font-semibold">
        Inspira UI<span :style="{ color: props.color }">.</span>
      </p>
    </BoxReveal>

    <BoxReveal
      :color="props.color"
      :duration="props.duration"
      :delay="props.delay"
    >
      <h2 class="mt-[.5rem] text-[1rem]">
        Beautiful components for
        <span :style="{ color: props.color }">Vue &amp; Nuxt.</span>
      </h2>
    </BoxReveal>

    <BoxReveal
      :color="props.color"
      :duration="props.duration"
      :delay="props.delay"
    >
      <div class="mt-6">
        <p>
          -&gt; Free and open-source animated components built with
          <span
            class="font-semibold"
            :style="{ color: props.color }"
          >
            Vue/Nuxt</span
          >,
          <span
            class="font-semibold"
            :style="{ color: props.color }"
          >
            Typescript</span
          >,
          <span
            class="font-semibold"
            :style="{ color: props.color }"
          >
            Tailwind CSS</span
          >, and
          <span
            class="font-semibold"
            :style="{ color: props.color }"
          >
            motion-v</span
          >
          . <br />
          -&gt; 100% open-source, and customizable. <br />
        </p>
      </div>
    </BoxReveal>
  </div>
</template>
