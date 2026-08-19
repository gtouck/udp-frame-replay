import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type UIEventHandler,
  type WheelEvent as ReactWheelEvent,
} from "react";

type Axis = "x" | "y";

type OverlayScrollAreaProps = {
  children: ReactNode;
  className?: string;
  onScroll?: UIEventHandler<HTMLDivElement>;
};

type DragState = {
  axis: Axis;
  pointerId: number;
  pointerStart: number;
  scrollStart: number;
};

const MIN_THUMB_SIZE = 24;
const SCROLL_IDLE_DELAY = 650;

const axisMetrics = (viewport: HTMLDivElement, axis: Axis) =>
  axis === "y"
    ? {
        clientSize: viewport.clientHeight,
        scrollSize: viewport.scrollHeight,
        scrollPosition: viewport.scrollTop,
      }
    : {
        clientSize: viewport.clientWidth,
        scrollSize: viewport.scrollWidth,
        scrollPosition: viewport.scrollLeft,
      };

const setScrollPosition = (
  viewport: HTMLDivElement,
  axis: Axis,
  position: number,
) => {
  if (axis === "y") viewport.scrollTop = position;
  else viewport.scrollLeft = position;
};

/**
 * 隐藏会占布局宽度的系统滚动条，用绝对定位的滑块保留可见反馈和拖动能力。
 * 真正滚动的仍是原生元素，因此滚轮、触控板、键盘和虚拟列表不受影响。
 */
const OverlayScrollArea = forwardRef<HTMLDivElement, OverlayScrollAreaProps>(
  function OverlayScrollArea({ children, className, onScroll }, forwardedRef) {
    const rootRef = useRef<HTMLDivElement>(null);
    const viewportRef = useRef<HTMLDivElement>(null);
    const verticalRailRef = useRef<HTMLDivElement>(null);
    const verticalThumbRef = useRef<HTMLDivElement>(null);
    const horizontalRailRef = useRef<HTMLDivElement>(null);
    const horizontalThumbRef = useRef<HTMLDivElement>(null);
    const frameRef = useRef<number | null>(null);
    const idleTimerRef = useRef<number | null>(null);
    const dragRef = useRef<DragState | null>(null);

    useImperativeHandle(forwardedRef, () => viewportRef.current!, []);

    const sync = useCallback(() => {
      const viewport = viewportRef.current;
      if (!viewport) return;

      const updateAxis = (
        axis: Axis,
        rail: HTMLDivElement | null,
        thumb: HTMLDivElement | null,
      ) => {
        if (!rail || !thumb) return;

        const { clientSize, scrollSize, scrollPosition } = axisMetrics(
          viewport,
          axis,
        );
        const trackSize =
          axis === "y" ? rail.clientHeight : rail.clientWidth;
        const visible = scrollSize - clientSize > 1 && trackSize > 0;

        rail.dataset.visible = visible ? "true" : "false";
        if (!visible) return;

        const thumbSize = Math.min(
          trackSize,
          Math.max(MIN_THUMB_SIZE, (trackSize * clientSize) / scrollSize),
        );
        const scrollRange = scrollSize - clientSize;
        const thumbRange = trackSize - thumbSize;
        const thumbPosition =
          scrollRange > 0 ? (scrollPosition / scrollRange) * thumbRange : 0;

        if (axis === "y") {
          thumb.style.height = `${thumbSize}px`;
          thumb.style.transform = `translate3d(0, ${thumbPosition}px, 0)`;
        } else {
          thumb.style.width = `${thumbSize}px`;
          thumb.style.transform = `translate3d(${thumbPosition}px, 0, 0)`;
        }
      };

      updateAxis("y", verticalRailRef.current, verticalThumbRef.current);
      updateAxis("x", horizontalRailRef.current, horizontalThumbRef.current);
    }, []);

    const scheduleSync = useCallback(() => {
      if (frameRef.current !== null) return;
      frameRef.current = requestAnimationFrame(() => {
        frameRef.current = null;
        sync();
      });
    }, [sync]);

    const showScrollingState = useCallback(() => {
      const root = rootRef.current;
      if (!root) return;

      root.dataset.scrolling = "true";
      if (idleTimerRef.current !== null) {
        window.clearTimeout(idleTimerRef.current);
      }
      idleTimerRef.current = window.setTimeout(() => {
        delete root.dataset.scrolling;
        idleTimerRef.current = null;
      }, SCROLL_IDLE_DELAY);
    }, []);

    useEffect(() => {
      const viewport = viewportRef.current;
      if (!viewport) return;

      const resizeObserver = new ResizeObserver(scheduleSync);
      resizeObserver.observe(viewport);

      // 列表追加、折叠面板开合和虚拟列表总高度变化都会改变 scrollSize，
      // 这些变化不一定触发容器自身的 ResizeObserver。
      const mutationObserver = new MutationObserver(scheduleSync);
      mutationObserver.observe(viewport, {
        attributes: true,
        childList: true,
        characterData: true,
        subtree: true,
      });

      void document.fonts?.ready.then(scheduleSync);

      // 窗口被遮挡或最小化时 requestAnimationFrame 不会触发，只走 scheduleSync
      // 的话滑块会一直停在初始的「不可见」上，等窗口回到前台也不会自己纠正。
      document.addEventListener("visibilitychange", sync);
      sync();

      return () => {
        resizeObserver.disconnect();
        mutationObserver.disconnect();
        document.removeEventListener("visibilitychange", sync);
        if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
        if (idleTimerRef.current !== null) {
          window.clearTimeout(idleTimerRef.current);
        }
      };
    }, [scheduleSync, sync]);

    const handleScroll: UIEventHandler<HTMLDivElement> = (event) => {
      scheduleSync();
      showScrollingState();
      onScroll?.(event);
    };

    const handleThumbPointerDown = (
      axis: Axis,
      event: ReactPointerEvent<HTMLDivElement>,
    ) => {
      const viewport = viewportRef.current;
      if (!viewport) return;

      event.preventDefault();
      event.stopPropagation();
      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = {
        axis,
        pointerId: event.pointerId,
        pointerStart: axis === "y" ? event.clientY : event.clientX,
        scrollStart:
          axis === "y" ? viewport.scrollTop : viewport.scrollLeft,
      };
      rootRef.current?.setAttribute("data-dragging", "true");
    };

    const handleThumbPointerMove = (
      event: ReactPointerEvent<HTMLDivElement>,
    ) => {
      const drag = dragRef.current;
      const viewport = viewportRef.current;
      if (!drag || !viewport || drag.pointerId !== event.pointerId) return;

      const rail =
        drag.axis === "y" ? verticalRailRef.current : horizontalRailRef.current;
      const thumb =
        drag.axis === "y"
          ? verticalThumbRef.current
          : horizontalThumbRef.current;
      if (!rail || !thumb) return;

      const trackSize =
        drag.axis === "y" ? rail.clientHeight : rail.clientWidth;
      const thumbSize =
        drag.axis === "y" ? thumb.offsetHeight : thumb.offsetWidth;
      const { clientSize, scrollSize } = axisMetrics(viewport, drag.axis);
      const pointer = drag.axis === "y" ? event.clientY : event.clientX;
      const thumbRange = trackSize - thumbSize;
      if (thumbRange <= 0) return;

      setScrollPosition(
        viewport,
        drag.axis,
        drag.scrollStart +
          ((pointer - drag.pointerStart) * (scrollSize - clientSize)) /
            thumbRange,
      );
    };

    const handleThumbPointerUp = (
      event: ReactPointerEvent<HTMLDivElement>,
    ) => {
      if (dragRef.current?.pointerId !== event.pointerId) return;
      dragRef.current = null;
      delete rootRef.current?.dataset.dragging;
    };

    const handleRailPointerDown = (
      axis: Axis,
      event: ReactPointerEvent<HTMLDivElement>,
    ) => {
      if (event.target !== event.currentTarget) return;

      const viewport = viewportRef.current;
      const thumb =
        axis === "y" ? verticalThumbRef.current : horizontalThumbRef.current;
      if (!viewport || !thumb) return;

      event.preventDefault();
      const rect = event.currentTarget.getBoundingClientRect();
      const trackSize = axis === "y" ? rect.height : rect.width;
      const thumbSize = axis === "y" ? thumb.offsetHeight : thumb.offsetWidth;
      const pointer =
        axis === "y" ? event.clientY - rect.top : event.clientX - rect.left;
      if (trackSize <= thumbSize) return;
      const ratio = Math.min(
        1,
        Math.max(0, (pointer - thumbSize / 2) / (trackSize - thumbSize)),
      );
      const { clientSize, scrollSize } = axisMetrics(viewport, axis);

      setScrollPosition(viewport, axis, ratio * (scrollSize - clientSize));
    };

    const handleRailWheel = (
      axis: Axis,
      event: ReactWheelEvent<HTMLDivElement>,
    ) => {
      const viewport = viewportRef.current;
      if (!viewport) return;

      const rawDelta =
        axis === "y"
          ? event.deltaY || event.deltaX
          : event.deltaX || event.deltaY;
      const delta =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? rawDelta * 16
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? rawDelta * (axis === "y" ? viewport.clientHeight : viewport.clientWidth)
            : rawDelta;
      setScrollPosition(
        viewport,
        axis,
        (axis === "y" ? viewport.scrollTop : viewport.scrollLeft) + delta,
      );
    };

    const rootClassName = className
      ? `overlay-scroll-area ${className}`
      : "overlay-scroll-area";

    return (
      <div className={rootClassName} ref={rootRef}>
        <div
          className="overlay-scroll-viewport"
          ref={viewportRef}
          onScroll={handleScroll}
        >
          {children}
        </div>

        <div
          aria-hidden="true"
          className="overlay-scrollbar"
          data-axis="y"
          data-visible="false"
          ref={verticalRailRef}
          onPointerDown={(event) => handleRailPointerDown("y", event)}
          onWheel={(event) => handleRailWheel("y", event)}
        >
          <div
            className="overlay-scroll-thumb"
            ref={verticalThumbRef}
            onPointerDown={(event) => handleThumbPointerDown("y", event)}
            onPointerMove={handleThumbPointerMove}
            onPointerUp={handleThumbPointerUp}
            onPointerCancel={handleThumbPointerUp}
          />
        </div>

        <div
          aria-hidden="true"
          className="overlay-scrollbar"
          data-axis="x"
          data-visible="false"
          ref={horizontalRailRef}
          onPointerDown={(event) => handleRailPointerDown("x", event)}
          onWheel={(event) => handleRailWheel("x", event)}
        >
          <div
            className="overlay-scroll-thumb"
            ref={horizontalThumbRef}
            onPointerDown={(event) => handleThumbPointerDown("x", event)}
            onPointerMove={handleThumbPointerMove}
            onPointerUp={handleThumbPointerUp}
            onPointerCancel={handleThumbPointerUp}
          />
        </div>
      </div>
    );
  },
);

export default OverlayScrollArea;
