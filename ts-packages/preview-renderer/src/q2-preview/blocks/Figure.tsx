import { useContext } from 'react';
import { Node, dataLocProps, inlinesToPlainText } from '../../framework';
import type { BlockNode, FigureBlock, InlineNode, NodeArgs } from '../../framework';
import { PreviewContext } from '../PreviewContext';

/** The inlines of a lone `Plain`/`Para`, else `null`. */
function loneRunOfInlines(blocks: BlockNode[]): InlineNode[] | null {
    if (blocks.length !== 1) return null;
    const [block] = blocks as any[];
    return block.t === 'Plain' || block.t === 'Para' ? (block.c as InlineNode[]) : null;
}

/**
 * Whether the figcaption only repeats the image's alt text (bd-ykjwiqcc
 * D5) — the twin of pampa's `figcaption_duplicates_alt`. Pandoc's rule:
 * for a figure holding a single image (or a link around one) whose alt
 * text equals the caption, the figcaption is hidden from assistive tech so
 * the text is announced once, and the `<img>` keeps its `alt`. The caption
 * is compared as written, so a crossref "Figure 1: …" caption stays
 * exposed. Alt is projected as `Image.tsx` renders it; the pipeline has
 * already folded any `alt`/`fig-alt` key-value into the alt inlines.
 */
function figcaptionDuplicatesAlt(bodyBlocks: BlockNode[], captionBlocks: BlockNode[]): boolean {
    const body = loneRunOfInlines(bodyBlocks);
    const caption = loneRunOfInlines(captionBlocks);
    if (!body || !caption || body.length !== 1) return false;
    let inline = body[0] as any;
    if (inline.t === 'Link') {
        const content = inline.c[1] as any[];
        if (content.length !== 1) return false;
        inline = content[0];
    }
    if (inline.t !== 'Image') return false;
    const alt = inlinesToPlainText(inline.c[1] as InlineNode[]).trim();
    return alt.length > 0 && alt === inlinesToPlainText(caption).trim();
}

/**
 * Figure → <figure> + optional <figcaption>. Reads `c[1][1]`
 * (captionBlocks) and `c[2]` (bodyBlocks) directly rather than going
 * through `renderChildren`, since renderChildren only emits c[2].
 *
 * Crossref-numbered captions ("Figure 1: ...") are already baked into
 * the caption blocks by `CrossrefResolveTransform` upstream — q2-preview
 * gets that for free.
 *
 * `setLocalAst` for body blocks updates `c[2]` immutably; for caption
 * blocks updates `c[1][1]`. Both go through the framework's atomic gate
 * via `<Node>`.
 */
export const Figure = (args: NodeArgs<FigureBlock>) => {
    const ctx = useContext(PreviewContext);
    const poolId = (args.node as any).s as string | number | undefined;
    const resolved = ctx?.resolveSource ? ctx.resolveSource(args.node) : null;
    const isEditable = resolved != null && resolved.reachabilityClass !== 'Opaque' && poolId !== undefined;

    const { node, setLocalAst, onNavigateToDocument } = args;
    const [[id, classes, kvs], [shortCaption, captionBlocks], bodyBlocks] = node.c;
    const props: Record<string, string | number> = {};
    if (id) props.id = id;
    if (classes.length) props.className = classes.join(' ');
    if (isEditable) {
        props['data-block-pool-id'] = String(poolId);
        props.tabIndex = -1;
    }

    // Float figcaption synthesis (bd-hcp8m3ve): the crossref renderer carries
    // figcaption metadata as `data-qf-*` kvs on the Figure attr (Pandoc's
    // Caption has no attr). Mirror pampa's HTML writer exactly: consume the
    // kvs (never emit them), give the figcaption its id + Q1-verbatim class
    // list, and honor top/bottom placement. Contract:
    // claude-notes/designs/float-layout-class-taxonomy.md.
    const kvMap = new Map(kvs ?? []);
    const qfCaptionId = kvMap.get('data-qf-caption-id');
    const qfLocation = kvMap.get('data-qf-caption-location') ?? 'bottom';
    const figcaptionProps: Record<string, string> = {};
    if (figcaptionDuplicatesAlt(bodyBlocks, captionBlocks)) {
        figcaptionProps['aria-hidden'] = 'true';
    }
    if (qfCaptionId !== undefined) {
        const refType = kvMap.get('data-qf-ref-type') ?? 'fig';
        let captionClasses =
            `quarto-float-caption-${qfLocation} quarto-float-caption quarto-float-${refType}`;
        if (kvMap.has('data-qf-uncaptioned')) captionClasses += ' quarto-uncaptioned';
        figcaptionProps.id = qfCaptionId;
        figcaptionProps.className = captionClasses;
    }

    const body = bodyBlocks.map((b, i) => (
        <Node
            key={i}
            node={b}
            onNavigateToDocument={onNavigateToDocument}
            setLocalAst={(newBlock: BlockNode | InlineNode) => {
                const next = bodyBlocks.slice();
                next[i] = newBlock as BlockNode;
                setLocalAst({ ...node, c: [node.c[0], node.c[1], next] });
            }}
        />
    ));
    const figcaption = captionBlocks.length > 0 && (
        <figcaption {...figcaptionProps}>
            {captionBlocks.map((b, i) => (
                <Node
                    key={i}
                    node={b}
                    onNavigateToDocument={onNavigateToDocument}
                    setLocalAst={(newBlock: BlockNode | InlineNode) => {
                        const next = captionBlocks.slice();
                        next[i] = newBlock as BlockNode;
                        setLocalAst({
                            ...node,
                            c: [node.c[0], [shortCaption, next], node.c[2]],
                        });
                    }}
                />
            ))}
        </figcaption>
    );

    return (
        <figure {...props} {...dataLocProps(node)}>
            {qfCaptionId !== undefined && qfLocation === 'top' ? (
                <>
                    {figcaption}
                    {body}
                </>
            ) : (
                <>
                    {body}
                    {figcaption}
                </>
            )}
        </figure>
    );
};
