export default {
    root: ({ props }) => ({
        class: [
            // Sizing and Shape
            'min-w-[12rem]',
            'rounded-lg',

            // Colors
            'bg-bg-two',
            'text-text-color',
            'border-2',
            { 'border-border-one': !props.invalid },

            // Invalid State
            { 'border-red': props.invalid },
        ],
    }),
    listContainer: 'overflow-auto',
    list: {
        class: 'py-2 list-none m-0 outline-none',
    },
    option: ({ context }) => ({
        class: [
            'relative',

            // Font
            'font-normal',
            'leading-none',

            // Flex
            'flex items-center',

            // Position
            'relative',

            // Shape
            'border-0',
            'rounded-none',

            // Spacing
            'm-0',
            'py-2 px-4 pl-5',

            // Colors
            {
                // 'text-text-color-secondary/70': !context.focused && !context.selected,
                // 'bg-surface-hover': context.focused && !context.selected,
                '!text-text-color': context.selected,
                // 'hover:text-text-color': context.selected,
            },

            //States
            'text-text-color-secondary/70 hover:bg-surface-hover hover:text-text-color',
            // {
            //     'hover:bg-surface-hover':
            //         !context.focused && !context.selected,
            // },
            // { 'hover:bg-highlight-emphasis': context.selected },
            // 'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring focus-visible:ring-inset focus-visible:ring-primary-400/50 dark:focus-visible:ring-primary-300/50',

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
            'py-2 px-2',

            // Color
            'text-text-color',
            'bg-transparent',

            // Misc
            'cursor-auto',
        ],
    },
    optionCheckIcon: 'relative -ms-2 me-1.5 text-text-color w-4 h-4',
    header: {
        class: [
            // Spacing
            'py-2 px-4',
            'm-0',

            //Shape
            'border-b',
            'rounded-tl-lg',
            'rounded-tr-lg',

            // Color
            'text-text-color',
            'bg-bg-two',
            'border-border-one',

            '[&_[data-pc-name=pcfilter]]:w-full',
        ],
    },
    emptyMessage: {
        class: [
            // Font
            'leading-none',

            // Spacing
            'py-2 px-4',

            // Color
            'text-text-color',
            'bg-transparent',
        ],
    },
}
