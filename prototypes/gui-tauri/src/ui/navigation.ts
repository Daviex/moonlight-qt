import { Page } from './types';

export function activeDialogRoot(): HTMLElement | null {
  return document.getElementById('active-dialog');
}

export function focusableElements(root: ParentNode = document): HTMLElement[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'),
  ).filter((element) =>
    !element.hasAttribute('disabled') &&
    element.tabIndex !== -1 &&
    element.offsetParent !== null,
  );
}

export function moveFocus(delta: number) {
  const elements = focusableElements(activeDialogRoot() ?? document);
  if (elements.length === 0) {
    return;
  }

  const currentIndex = Math.max(0, elements.indexOf(document.activeElement as HTMLElement));
  const nextIndex = (currentIndex + delta + elements.length) % elements.length;
  elements[nextIndex].focus();
}

export function focusPreferredElement(root: ParentNode = document) {
  const preferredElement = root.querySelector<HTMLElement>('[data-controller-focus="true"]');
  if (preferredElement &&
      !preferredElement.hasAttribute('disabled') &&
      preferredElement.tabIndex !== -1 &&
      preferredElement.offsetParent !== null) {
    preferredElement.focus();
    return;
  }

  focusableElements(root)[0]?.focus();
}

export function focusPage(page: Page) {
  focusPreferredElement(document.querySelector<HTMLElement>(`[data-page-panel="${page}"]`) ?? document);
}

export function focusCardActions() {
  const activeElement = document.activeElement;
  if (!(activeElement instanceof HTMLElement)) {
    return false;
  }

  const card = activeElement.closest<HTMLElement>('[data-controller-card="true"]');
  if (!card) {
    return false;
  }

  const actionElements = focusableElements(card.querySelector<HTMLElement>('.card-actions') ?? card);
  if (actionElements.length === 0) {
    return false;
  }

  actionElements[0].focus();
  return true;
}
