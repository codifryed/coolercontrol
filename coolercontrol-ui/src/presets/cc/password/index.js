export default {
    root: ({ props }) => ({
        class: [
            'relative',
            { 'flex [&>input]:w-full': props.fluid, 'inline-flex': !props.fluid },
            {
                'opacity-60 select-none pointer-events-none cursor-default': props.disabled
            },
            { '[&>input]:pr-10': props.toggleMask }
        ]
    }),
    overlay: {
        class: [
            // Spacing
            'p-5',

            // Shape
            'border-0 dark:border',
            'shadow-md rounded-md',

            // Colors
            'bg-surface-0 dark:bg-surface-900',
            'text-surface-700 dark:text-white/80',
            'dark:border-surface-700'
        ]
    },
    meter: {
        class: [
            // Position and Overflow
            'overflow-hidden',
            'relative',

            // Shape and Size
            'border-0',
            'h-3',

            // Spacing
            'mb-2',

            // Colors
            'bg-surface-100 dark:bg-surface-700'
        ]
    },
    meterLabel: ({ instance }) => ({
        class: [
            // Size
            'h-full',

            // Colors
            {
                'bg-red-500 dark:bg-red-400/50': instance?.meter?.strength == 'weak',
                'bg-orange-500 dark:bg-orange-400/50': instance?.meter?.strength == 'medium',
                'bg-green-500 dark:bg-green-400/50': instance?.meter?.strength == 'strong'
            },

            // Transitions
            'transition-all duration-1000 ease-in-out'
        ]
    }),
    maskIcon: {
        class: ['absolute top-1/2 right-3 -mt-2 z-10', 'text-surface-600 dark:text-white/70']
    },
    unmaskIcon: {
        class: ['absolute top-1/2 right-3 -mt-2 z-10', 'text-surface-600 dark:text-white/70']
    },
    transition: {
        enterFromClass: 'opacity-0 scale-y-[0.8]',
        enterActiveClass: 'transition-[transform,opacity] duration-[120ms] ease-[cubic-bezier(0,0,0.2,1)]',
        leaveActiveClass: 'transition-opacity duration-100 ease-linear',
        leaveToClass: 'opacity-0'
    }
};
