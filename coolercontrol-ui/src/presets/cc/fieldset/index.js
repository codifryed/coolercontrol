export default {
    root: {
        class: [
            'block',

            // Spacing
            'px-4 py-0',
            'inline-size-min',

            // Shape
            'rounded-lg',
            // Color
            'border-0 border-border-one',
            'bg-bg-two',
            'text-text-color',
        ],
    },
    legend: ({ props }) => ({
        class: [
            // Font
            'font-bold',
            'leading-none',

            //Spacing
            { 'p-0': props.toggleable, 'py-2 pl-2 px-3': !props.toggleable },

            // Shape
            'rounded-lg',

            // Color
            'text-text-color',
            'border border-border-one',
            'bg-bg-two',

            // Transition
            'transition-none',

            // States
            {
                'hover:bg-surface-100 hover:border-surface-200 hover:text-surface-900 dark:hover:text-surface-0/80 dark:hover:bg-surface-800/80':
                    props.toggleable,
            },
        ],
    }),
    toggleButton: ({ props }) => ({
        class: [
            // Alignments
            'flex items-center justify-center',
            'relative',

            //Spacing
            { 'p-5': props.toggleable },

            // Shape
            { 'rounded-lg': props.toggleable },

            // Color
            { 'text-surface-700 dark:text-surface-200 hover:text-surface-900': props.toggleable },

            // States
            { 'hover:text-surface-900 dark:hover:text-surface-100': props.toggleable },
            {
                'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring focus-visible:ring-inset focus-visible:ring-primary-400/50 dark:focus-visible:ring-primary-300/50':
                    props.toggleable,
            },

            // Misc
            {
                'transition-none cursor-pointer overflow-hidden select-none': props.toggleable,
            },
        ],
    }),
    toggleIcon: {
        class: 'mr-2 inline-block',
    },
    legendLabel: {
        class: 'flex items-center justify-center leading-none',
    },
    content: {
        class: 'pt-4 pb-2',
    },
    transition: {
        enterFromClass: 'max-h-0',
        enterActiveClass:
            'overflow-hidden transition-[max-height] duration-1000 ease-[cubic-bezier(0.42,0,0.58,1)]',
        enterToClass: 'max-h-[1000px]',
        leaveFromClass: 'max-h-[1000px]',
        leaveActiveClass:
            'overflow-hidden transition-[max-height] duration-[450ms] ease-[cubic-bezier(0,1,0,1)]',
        leaveToClass: 'max-h-0',
    },
}
