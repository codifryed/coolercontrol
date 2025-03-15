export default {
    root: {
        class: [
            'relative',

            // Alignment
            'inline-flex',
            'align-bottom',

            // Size
            'w-6',
            'h-6',

            // Misc
            'cursor-pointer',
            'select-none',
        ],
    },
    box: ({ props, context }) => ({
        class: [
            // Alignment
            'flex',
            'items-center',
            'justify-center',

            // Size
            'w-6',
            'h-6',

            // Shape
            'rounded-lg',
            // 'border border-border-one',
            'bg-surface-hover',

            // Colors
            // {
            //     'border-border-one bg-surface-hover': !context.checked && !props.invalid,
            //     'border-border-one bg-bg-two': context.checked,
            // },

            // Invalid State
            { 'border-red': props.invalid },

            // States
            {
                'peer-hover:border-border-one':
                    !props.disabled && !context.checked && !props.invalid,
                'peer-hover:bg-surface-hover peer-hover:border-border-one':
                    !props.disabled && context.checked,
                // 'peer-focus-visible:border-primary-500 dark:peer-focus-visible:border-primary-400 peer-focus-visible:ring-2 peer-focus-visible:ring-primary-400/20 dark:peer-focus-visible:ring-primary-300/20':
                //     !props.disabled,
                'cursor-default opacity-60': props.disabled,
            },

            // Transitions
            'transition-colors',
            'duration-200',
        ],
    }),
    input: {
        class: [
            'peer',

            // Size
            'w-full ',
            'h-full',

            // Position
            'absolute',
            'top-0 left-0',
            'z-10',

            // Spacing
            'p-0',
            'm-0',

            // Shape
            'opacity-0',
            'rounded-lg',
            'outline-none',
            // 'border border-text-color-secondary',

            // Misc
            'appearance-none',
            'cursor-pointer',
        ],
    },
    icon: ({ state, context }) => ({
        class: [
            // Font
            'text-base leading-none',

            // Size
            'w-4',
            'h-4',

            // Colors
            {
                'text-text-color': context.checked,
                'text-text-color-secondary': state.d_indeterminate,
            },

            // Transitions
            'transition-all',
            'duration-200',
        ],
    }),
}
