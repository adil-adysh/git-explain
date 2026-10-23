<script lang="ts">
  // HelpPage: documents keyboard interaction and the non-negotiable inference rule
</script>

<section aria-labelledby="destination-heading" class="page-container">
  <h1 id="destination-heading" tabindex="-1">Help &amp; Keyboard Guide</h1>
  <p class="page-description">
    Essential principles, keyboard navigation instructions, and workspace reference for Git Explain.
  </p>

  <div class="help-section">
    <h2>Choose a file and read a code section</h2>
    <p>Selecting a file opens its first code section. Returning to a file restores the last section you read in this workspace. The complete section list stays available. Previous and Next stay within that file. Files with no code sections do not load source.</p>
  </div>

  <div class="help-section">
    <h2>Generate only when requested</h2>
    <p>
      <strong>Navigation is not generation.</strong> Exploring repository files, selecting code sections, changing filters, traversing browser history, switching explanation views, adjusting settings, and loading saved explanations will <em>never</em> contact an AI model.
    </p>
    <p>
      Explanations are generated <strong>only</strong> after you explicitly activate the <strong>Generate</strong> or <strong>Regenerate</strong> action for an eligible code section.
    </p>
  </div>

  <div class="help-section">
    <h2>Keyboard Navigation Matrix</h2>
    <p>
      Git Explain uses standard controls without custom letter shortcuts to preserve screen reader and browser accessibility.
    </p>
    <table class="keyboard-table">
      <thead>
        <tr>
          <th scope="col">Key / Shortcut</th>
          <th scope="col">Context</th>
          <th scope="col">Action</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td><kbd>Tab</kbd> / <kbd>Shift</kbd> + <kbd>Tab</kbd></td>
          <td>Anywhere</td>
          <td>Move focus forward / backward in natural document order</td>
        </tr>
        <tr>
          <td><kbd>Enter</kbd></td>
          <td>Destination links &amp; files</td>
          <td>Navigate to destination or select file / section</td>
        </tr>
        <tr>
          <td><kbd>Space</kbd> / <kbd>Enter</kbd></td>
          <td>Buttons</td>
          <td>Activate the button action</td>
        </tr>
        <tr>
          <td><kbd>Left</kbd> / <kbd>Right</kbd></td>
          <td>Explanation tabs</td>
          <td>Move focus between tabs without activating or triggering generation</td>
        </tr>
        <tr>
          <td><kbd>Home</kbd> / <kbd>End</kbd></td>
          <td>Explanation tabs</td>
          <td>Move focus to the first / last explanation tab</td>
        </tr>
        <tr>
          <td><kbd>Enter</kbd> / <kbd>Space</kbd></td>
          <td>Focused explanation tab</td>
          <td>Activate tab to view a saved result (does not call model)</td>
        </tr>
        <tr>
          <td><kbd>Alt</kbd> + <kbd>Left</kbd> (Browser Back)</td>
          <td>Browser</td>
          <td>Traverse back to previous section or destination without generating</td>
        </tr>
      </tbody>
    </table>
  </div>

  <div class="help-section">
    <h2>Workspace Destinations</h2>
    <ul>
      <li><strong>Explore:</strong> Browse repository files and code sections, read source and diffs, and generate explanations.</li>
      <li><strong>Personalization:</strong> Configure saved learning profiles and concept preferences.</li>
      <li><strong>Settings:</strong> Customize theme, font scale, code wrapping, and inspect read-only runtime diagnostics.</li>
      <li><strong>Help:</strong> Access this keyboard guide at any time.</li>
    </ul>
  </div>
</section>
