import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      reportsDirectory: 'coverage',
      include: ['src/**/*.ts'],
      // The WebKit bootstrap and controller are browser-integration code. The
      // covered modules below hold their deterministic document transforms.
      exclude: [
        'src/**/*.d.ts',
        'src/main.ts',
        'src/editor/editor-controller.ts',
        'src/editor/protocol.ts',
      ],
      thresholds: {
        branches: 85,
        functions: 90,
        lines: 90,
        statements: 90,
      },
    },
  },
});
