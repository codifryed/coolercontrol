export default {
    root: ({ props, context }) => ({
        class: [
            'relative shrink-0',
            // Transitions
            'transition-all duration-200',

            // Misc
            'cursor-pointer select-none whitespace-nowrap',
            'user-select-none',

            // Shape
            'border-b-4 border-border-one',
            // 'rounded-t-lg',

            // Spacing
            'py-4 px-[1.125rem]',
            '-mb-px',

            // Colors and Conditions
            'outline-transparent',
            {
                'border-border-one border-b-2': !context.active,
                'bg-bg-one': !context.active,
                'text-text-color-secondary': !context.active,

                // 'bg-bg-one': context.active,
                'border-text-color-secondary border-b-4': context.active,
                'text-text-color': context.active,

                'opacity-30 cursor-default user-select-none select-none pointer-events-none':
                    props?.disabled,
            },
            // States
            'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring focus-visible:ring-inset',
            'focus-visible:ring-primary-400/50',
            {
                'hover:bg-surface-hover': !context.active,
                'hover:text-text-color': !context.active,
            },
        ],
    }),
}
