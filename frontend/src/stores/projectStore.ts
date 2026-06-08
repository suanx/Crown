import { create } from "zustand";
import {
  agentClient,
  type CreateProjectInput,
  type ProjectSummary,
  type UpdateProjectInput,
} from "@/api";

interface ProjectState {
  projects: ProjectSummary[];
  loading: boolean;
  error: string | null;
  loadProjects: () => Promise<void>;
  pickProjectDirectory: () => Promise<string | null>;
  createProject: (input: CreateProjectInput) => Promise<ProjectSummary>;
  updateProject: (input: UpdateProjectInput) => Promise<void>;
  deleteProject: (projectId: string) => Promise<void>;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projects: [],
  loading: false,
  error: null,

  loadProjects: async () => {
    set({ loading: true, error: null });
    try {
      const projects = await agentClient.listProjects();
      set({ projects, loading: false });
    } catch (err) {
      set({
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  pickProjectDirectory: async () => agentClient.pickProjectDirectory(),

  createProject: async (input) => {
    const project = await agentClient.createProject(input);
    set({ projects: [project, ...get().projects], error: null });
    void get().loadProjects();
    return project;
  },

  updateProject: async (input) => {
    await agentClient.updateProject(input);
    set({
      projects: get().projects.map((project) =>
        project.id === input.projectId
          ? {
              ...project,
              ...(input.name !== undefined ? { name: input.name } : {}),
              ...(input.path !== undefined ? { path: input.path } : {}),
            }
          : project,
      ),
      error: null,
    });
    void get().loadProjects();
  },

  deleteProject: async (projectId) => {
    await agentClient.deleteProject(projectId);
    set({
      projects: get().projects.filter((project) => project.id !== projectId),
      error: null,
    });
  },
}));
