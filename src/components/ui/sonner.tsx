import { useTheme } from "next-themes"
import { Toaster as Sonner, toast as sonnerToast } from "sonner"

type ToasterProps = React.ComponentProps<typeof Sonner>

export const toast = sonnerToast

const Toaster = () => {
  const { theme = "system" } = useTheme()

  return (
<Sonner 
	theme={theme as ToasterProps["theme"]} 
	className="toaster group" 
	richColors 
	position="bottom-center" 
 />
  )
}

export { Toaster }
