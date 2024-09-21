export default {
    root: ({ props, state }) => ({
        class: [
            // Display and Position
            'inline-flex',
            'relative',

            // Shape
            'rounded-lg',

            // Color and Background
            'text-text-color',
            'bg-bg-one',
            'border-2',
            'border-border-one',
            { 'border-border-one': !props.invalid },

            // Invalid State
            { 'border-red': props.invalid },

            // Transitions
            // 'transition-all',
            // 'duration-200',

            // States
            { 'hover:bg-surface-hover': !props.invalid },
            {
                'outline-none outline-offset-0 ring-1 ring-primary-500/50': state.focused,
            },
            'focus:outline-none focus:outline-offset-0 focus:ring-1',

            // Misc
            'cursor-pointer',
            'select-none',
            {
                'opacity-60': props.disabled,
                'pointer-events-none': props.disabled,
                'cursor-default': props.disabled,
            },
        ],
    }),
    labelContainer: 'overflow-hidden flex flex-auto cursor-pointer',
    label: ({ props }) => ({
        class: [
            'leading-[normal]',
            'block ',

            // Spacing
            {
                'p-2': props.display !== 'chip',
                'py-2 px-2': props.display === 'chip' && !props?.modelValue?.length,
                'py-[0.375rem] px-3': props.display === 'chip' && props?.modelValue?.length > 0,
            },

            // Color
            'text-text-color',
            // {
            //     'text-surface-800 dark:text-white/80': props.modelValue?.length,
            //     'text-surface-400 dark:text-surface-500': !props.modelValue?.length,
            // },
            'placeholder:text-text-color-secondary',

            // Transitions
            'transition duration-200',

            // Misc
            'overflow-hidden whitespace-nowrap cursor-pointer overflow-ellipsis',
        ],
    }),
    dropdown: {
        class: [
            // Flexbox
            'flex items-center justify-center',
            'shrink-0',

            // Color and Background
            'text-text-color-secondary hover:text-text-color',
            'bg-transparent',
            'border-border-one',

            // Size
            'w-6 pr-2',

            // Shape
            'rounded-tr-lg',
            'rounded-br-lg',
        ],
    },
    overlay: {
        class: [
            // Position

            // Shape
            'border-0',
            'rounded-lg',
            'shadow-lg',

            // Color
            'text-text-color-secondary',
            'bg-bg-two',
            'border-border-one',
        ],
    },
    header: {
        class: [
            'flex items-center justify-between',
            // Spacing
            'py-2 px-5 gap-2',
            'm-0',

            //Shape
            'border-b-2',
            'rounded-tl-lg',
            'rounded-tr-lg',

            // Color
            'text-text-color',
            'bg-transparent',
            'border-border-one',

            '[&_[data-pc-name=pcfiltercontainer]]:!flex-auto',
            '[&_[data-pc-name=pcfilter]]:w-full',
        ],
    },
    listContainer: {
        class: [
            // Sizing
            'max-h-[400px]',

            // Misc
            'overflow-auto',
        ],
    },
    list: {
        class: 'py-2 list-none m-0',
    },
    option: ({ context }) => ({
        class: [
            // Font
            'font-normal',
            'leading-none',

            // Flexbox
            'flex items-center',

            // Position
            'relative',

            // Shape
            'border-0',
            'rounded-none',

            // Spacing
            'm-0',
            'py-2 px-5 gap-2',

            // Color
            { 'text-text-color-secondary': !context.focused && !context.selected },
            {
                'bg-surface-hover': context.focused && !context.selected,
                'text-text-color': (context.focused && !context.selected) || context.selected,
            },
            { 'hover:text-text-color': context.selected },

            //States
            'hover:bg-surface-hover',
            // {
            //     'hover:bg-surface-hover':
            //         !context.focused && !context.selected,
            // },

            // Transitions
            'transition-shadow',
            'duration-200',

            // Misc
            'cursor-pointer',
            'overflow-hidden',
            'whitespace-nowrap',
        ],
    }),
    optionGroup: {
        class: [
            //Font
            'font-bold',

            // Spacing
            'm-0',
            'p-2 px-5',

            // Color
            'text-text-color',
            'bg-transparent',

            // Misc
            'cursor-auto',
        ],
    },
    emptyMessage: {
        class: [
            // Font
            'leading-none',

            // Spacing
            'py-2 px-5',

            // Color
            'text-text-color',
            'bg-transparent',
        ],
    },
    loadingIcon: {
        class: 'text-text-color-secondary animate-spin',
    },
    transition: {
        enterFromClass: 'opacity-0 scale-y-[0.8]',
        enterActiveClass:
            'transition-[transform,opacity] duration-[120ms] ease-[cubic-bezier(0,0,0.2,1)]',
        leaveActiveClass: 'transition-opacity duration-100 ease-linear',
        leaveToClass: 'opacity-0',
    },
}
