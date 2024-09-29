export default {
    root: ({ props }) => ({
        class: [
            // Misc
            { 'opacity-60 select-none pointer-events-none cursor-default': props.disabled },
        ],
    }),
    range: {
        class: [
            // Stroke
            // 'stroke-current',

            // Color
            'stroke-bg-two',

            // Fill
            'fill-none',

            // Transition
            'transition duration-100 ease-in',
        ],
    },
    value: {
        class: [
            // Animation
            'animate-dash-frame',

            // Color
            'stroke-accent',

            // Fill
            'fill-none',
        ],
    },
    text: {
        class: [
            // Text Style
            'text-center text-xl',

            // Color
            'fill-text-color',
        ],
    },
}
