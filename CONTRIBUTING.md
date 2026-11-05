# Contributing to Videleum

First off, thanks for taking the time to contribute! 🎉

The following is a set of guidelines for contributing to Videleum. Use your best judgment, and feel free to propose changes in a pull request.

## Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:
- [Node.js](https://nodejs.org/) (22.22+, 23.x, 24.x, 25.x, or 26.x)
- [npm](https://www.npmjs.com/) (v10 or higher)
- [Rust](https://rustup.rs/) (1.80+, for GPUI desktop app)

### Installation

1. **Fork the repository** on GitHub.
2. **Clone your fork** locally:
    ```bash
    git clone https://github.com/your-username/Videleum.git
    cd Videleum
    ```
3. **Install dependencies** for both frontend and backend:
    ```bash
    npm run install:all
    ```

### Running Locally

To start the development environment (both frontend and backend):

```bash
npm run dev
```

- **Frontend**: http://localhost:5556
- **Backend API**: http://localhost:5551

To run or build the desktop app:

```bash
cd desktop
cargo run
```

## Project Structure

- `desktop/`: Native desktop application built with Rust GPUI + GPUI Kit.
- `frontend/`: Web UI built with React + Vite + TypeScript.
- `backend/`: API & background downloader service built with Express.js + TypeScript + FFmpeg + SQLite.

## Development Workflow

1. **Create a Branch**: Always work on a new branch for your changes.
    ```bash
    git checkout -b feature/my-awesome-feature
    ```
2. **Make Changes**: Implement your feature or fix.
3. **Commit**: Write clear, descriptive commit messages.
    ```bash
    git commit -m "feat: add new video player controls"
    ```

## Code Quality

- Frontend lint & tests:
    ```bash
    cd frontend && npm run lint && npm test
    ```
- Backend tests:
    ```bash
    cd backend && npm test
    ```
- Desktop tests:
    ```bash
    cd desktop && cargo test
    ```

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
