export default {
    root: ({ props, state, parent }) => ({
        class: [
            // Display and Position
            // 'inline-flex',
            'flex justify-between',
            'relative',

            // Shape
            { 'rounded-lg': parent.instance.$name !== 'InputGroup' },
            {
                'first:rounded-l-lg rounded-none last:rounded-r-lg':
                    parent.instance.$name == 'InputGroup',
            },
            { 'border-0 border-y border-l last:border-r': parent.instance.$name == 'InputGroup' },
            { 'first:ml-0 ml-[-1px]': parent.instance.$name == 'InputGroup' && !props.showButtons },

            // Color and Background
            'text-text-color',
            'bg-bg-one',
            'border-2',
            'border-border-one',
            // { 'dark:border-surface-700': parent.instance.$name != 'InputGroup' },
            // { 'dark:border-surface-600': parent.instance.$name == 'InputGroup' },
            { 'border-border-one': !props.invalid },

            // Invalid State
            { 'border-red': props.invalid },

            // Transitions
            // Causes a border flash from the main TW styling... disabled
            // 'transition-all',
            // 'duration-200',

            // States
            { 'hover:bg-surface-hover': !props.invalid },
            {
                // 'outline-none outline-offset-0 ring ring-primary-400/50':
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
    label: ({ props, parent }) => ({
        class: [
            //Font
            'leading-[normal]',

            // Display
            'block',
            // 'flex-auto',

            // Color and Background
            'bg-transparent',
            'border-0',
            'text-text-color',
            // {
            //     'text-surface-800': props.modelValue != undefined,
            //     'text-surface-400': props.modelValue == undefined,
            // },
            'placeholder:text-text-color-secondary',

            // Sizing and Spacing
            // 'w-[1%]',
            'p-2',
            { 'pr-7': props.showClear },

            //Shape
            'rounded-none',

            // Transitions
            'transition',
            'duration-200',

            // States
            'focus:outline-none focus:shadow-none',

            // Filled State *for FloatLabel
            { filled: parent.instance?.$name == 'FloatLabel' && props.modelValue !== null },

            // Misc
            'relative',
            'cursor-pointer',
            'overflow-hidden overflow-ellipsis',
            'whitespace-nowrap',
            'appearance-none',
        ],
    }),
    dropdown: {
        class: [
            // Flexbox
            // 'flex items-center justify-center',
            'shrink-0',
            'flex items-center content-center',

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
            // 'absolute top-0 left-0',

            // Shape
            'border-0',
            'rounded-lg',
            'shadow-lg',

            // Color
            'text-text-color-secondary',
            'bg-bg-two',
            'border-border-one',
            // 'bg-surface-0',
        ],
    },
    listContainer: {
        class: [
            // Sizing
            // 'max-h-[400px]',

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

            // Position
            'relative',
            'flex items-center',

            // Shape
            'border-0',
            'border-border-one',
            'rounded-none',

            // Spacing
            'm-0',
            'py-2 px-6',

            // Colors
            {
                'bg-surface-hover': context.focused && !context.selected,
                'text-text-color': (context.focused && !context.selected) || context.selected,
            },

            //States
            'hover:bg-surface-hover',
            // {
            //     'hover:bg-surface-hover':
            //         !context.focused && !context.selected,
            // },
            { 'hover:text-text-color': context.selected },
            // 'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring',
            // 'focus-visible:ring-inset focus-visible:ring-primary-400/50',

            // Transitions
            'transition-shadow',
            'duration-200',

            // Misc
            { 'pointer-events-none cursor-default': context.disabled },
            { 'cursor-pointer': !context.disabled },
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
            'py-2 px-4',

            // Color
            'text-text-color',
            'bg-transparent',

            // Misc
            'cursor-auto',
        ],
    },
    optionCheckIcon: 'relative -ms-1.5 me-1.5 text-text-color w-4 h-4',
    optionBlankIcon: 'w-4 h-4',
    emptyMessage: {
        class: [
            // Font
            'leading-none',

            // Spacing
            'py-2 px-2',

            // Color
            // 'text-surface-800',
            'bg-transparent',
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
    clearIcon: {
        class: [
            // Color
            'text-surface-500',

            // Position
            'absolute',
            'top-1/2',
            'right-12',

            // Spacing
            '-mt-2',
        ],
    },
    loadingIcon: {
        class: 'text-surface-400 animate-spin',
    },
    transition: {
        enterFromClass: 'opacity-0 scale-y-[0.8]',
        enterActiveClass:
            'transition-[transform,opacity] duration-[120ms] ease-[cubic-bezier(0,0,0.2,1)]',
        leaveActiveClass: 'transition-opacity duration-100 ease-linear',
        leaveToClass: 'opacity-0',
    },
}
