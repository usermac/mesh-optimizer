# Blog Images

Place image files in this directory. They will be deployed automatically with `./deploy_blog.sh` or `./deploy.sh`.

## Usage

Reference images in blog articles using absolute paths:

```html
<img src="/blog/images/your-image.png" alt="Description of the image">
```

## Recommended Practices

- **Formats**: WebP (preferred), PNG, JPG, SVG
- **Naming**: Use kebab-case (`workflow-diagram.png`, not `Workflow Diagram.png`)
- **Size**: Optimize images before adding (aim for < 200KB per image)
- **Alt text**: Always include descriptive alt text for accessibility

## Example

```html
<figure>
  <img src="/blog/images/meshopt-workflow.png" alt="Mesh optimization workflow showing input model, processing steps, and optimized output">
  <figcaption>The mesh optimization pipeline</figcaption>
</figure>
```
