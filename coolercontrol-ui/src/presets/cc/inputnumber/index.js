export default {
    root: ({ props, parent }) => ({
        class: [
            // Flex
            'inline-flex',
            'relative',
            { 'flex-col': props.showButtons && props.buttonLayout === 'vertical' },
            { 'flex-1 w-[1%]': parent.instance.$name === 'InputGroup' },
            { 'w-full': props.fluid },

            // Shape
            {
                'first:rounded-l-lg rounded-none last:rounded-r-lg':
                    parent.instance.$name === 'InputGroup' && !props.showButtons,
            },
            {
                'border-0 border-y border-l last:border-r border-surface-300':
                    parent.instance.$name === 'InputGroup' && !props.showButtons,
            },
            { 'first:ml-0 -ml-px': parent.instance.$name === 'InputGroup' && !props.showButtons },

            //Sizing
            { '!w-16': props.showButtons && props.buttonLayout === 'vertical' },
        ],
    }),
    pcInputText: {
        root: ({ parent, context }) => ({
            class: [
                // Font
                // 'text-base leading-none',
                // 'leading-[normal]',

                // Display
                'flex-auto',
                { 'w-[1%]': parent.props.fluid },
                // 'w-24', // input size determines full width with buttons

                //Text
                {
                    'text-center':
                        parent.props.showButtons &&
                        (parent.props.buttonLayout === 'vertical' ||
                            parent.props.buttonLayout === 'horizontal'),
                },

                // Spacing
                'py-0 px-2',
                'm-0',

                // Shape
                'rounded-lg',
                {
                    'rounded-l-none rounded-r-none':
                        parent.props.showButtons && parent.props.buttonLayout === 'horizontal',
                },
                {
                    'rounded-none':
                        parent.props.showButtons && parent.props.buttonLayout === 'vertical',
                },

                {
                    'border-0':
                        parent.instance.$parentInstance?.$name === 'InputGroup' &&
                        !parent.props.showButtons,
                },

                // Colors
                'text-text-color',
                'placeholder:text-text-color-secondary',
                { 'bg-bg-one': !context.disabled },
                'border-2',
                { 'border-border-one': !parent.props.invalid },

                // Invalid State
                'invalid:focus:ring-red',
                'invalid:hover:border-red',
                { 'border-red': parent.props.invalid },

                // States
                'hover:bg-surface-hover',
                // { 'hover:border-border-one': !parent.props.invalid },
                'focus:outline-none focus:outline-offset-0 focus:ring-1 focus:ring-primary-500/50 focus:z-11',
                { 'opacity-60 select-none pointer-events-none cursor-default': context.disabled },

                // Filled State *for FloatLabel
                {
                    filled:
                        parent.instance?.$parentInstance?.$name === 'FloatLabel' &&
                        parent.state.d_modelValue !== null,
                },

                //Position
                {
                    'order-2':
                        parent.props.buttonLayout === 'horizontal' ||
                        parent.props.buttonLayout === 'vertical',
                },
            ],
        }),
    },
    buttonGroup: ({ props }) => ({
        class: [
            'absolute',

            // Flex
            'flex',
            'flex-col',

            'top-px right-px',

            { 'h-[calc(100%-2px)]': props.showButtons && props.buttonLayout === 'stacked' },
        ],
    }),
    incrementButton: ({ props }) => ({
        class: [
            // Display
            {
                'flex flex-initial shrink-0':
                    props.showButtons && props.buttonLayout === 'horizontal',
            },
            { 'flex flex-auto': props.showButtons && props.buttonLayout === 'stacked' },

            // Alignment
            'items-center',
            'justify-center',
            'text-center align-bottom',

            //Position
            'relative',
            { 'order-3': props.showButtons && props.buttonLayout === 'horizontal' },
            { 'order-1': props.showButtons && props.buttonLayout === 'vertical' },

            //Color
            'text-text-color-secondary',
            // 'bg-transparent',
            // 'border-2',
            // 'border-border-one',
            {
                'border-2 border-border-one border-l-0':
                    props.showButtons && props.buttonLayout === 'horizontal',
            },

            // Sizing
            'w-[2rem]',
            { 'px-4 py-2': props.showButtons && props.buttonLayout !== 'stacked' },
            { 'p-0': props.showButtons && props.buttonLayout === 'stacked' },
            { 'w-full': props.showButtons && props.buttonLayout === 'vertical' },

            // Shape
            'rounded-lg',
            {
                'rounded-tl-none rounded-br-none rounded-bl-none':
                    props.showButtons && props.buttonLayout === 'stacked',
            },
            {
                'rounded-bl-none rounded-tl-none':
                    props.showButtons && props.buttonLayout === 'horizontal',
            },
            {
                'rounded-bl-none rounded-br-none':
                    props.showButtons && props.buttonLayout === 'vertical',
            },

            //States
            'focus:outline-none focus:outline-offset-0 focus:ring',
            'hover:bg-surface-hover hover:text-text-color',

            //Misc
            'cursor-pointer overflow-hidden select-none',
        ],
    }),
    incrementIcon: 'inline-block w-4 h-4',
    decrementButton: ({ props }) => ({
        class: [
            // Display
            {
                'flex flex-initial shrink-0':
                    props.showButtons && props.buttonLayout === 'horizontal',
            },
            { 'flex flex-auto': props.showButtons && props.buttonLayout === 'stacked' },

            // Alignment
            'items-center',
            'justify-center',
            'text-center align-bottom',

            //Position
            'relative',
            { 'order-1': props.showButtons && props.buttonLayout === 'horizontal' },
            { 'order-3': props.showButtons && props.buttonLayout === 'vertical' },

            //Color
            'text-text-color-secondary',
            // 'bg-transparent',
            // 'border-border-one',
            {
                'border-2 border-border-one border-r-0':
                    props.showButtons && props.buttonLayout === 'horizontal',
            },

            // Sizing
            'w-[2rem]',
            { 'px-4 py-2': props.showButtons && props.buttonLayout !== 'stacked' },
            { 'p-0': props.showButtons && props.buttonLayout === 'stacked' },
            { 'w-full': props.showButtons && props.buttonLayout === 'vertical' },

            // Shape
            'rounded-lg',
            {
                'rounded-tr-none rounded-tl-none rounded-bl-none':
                    props.showButtons && props.buttonLayout === 'stacked',
            },
            {
                'rounded-tr-none rounded-br-none ':
                    props.showButtons && props.buttonLayout === 'horizontal',
            },
            {
                'rounded-tr-none rounded-tl-none ':
                    props.showButtons && props.buttonLayout === 'vertical',
            },

            //States
            'focus:outline-none focus:outline-offset-0 focus:ring',
            'hover:bg-surface-hover hover:text-text-color',

            //Misc
            'cursor-pointer overflow-hidden select-none',
        ],
    }),
    decrementIcon: 'inline-block w-4 h-4',
}
